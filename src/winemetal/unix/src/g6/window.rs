//! Pull-on-present window, layer, display, and presented-event Unix calls (slots 158 to 163).
//!
//! Owns `WMTDeclareLatencyCriticalActivity`, `WMTAttachMetalView`, `WMTRetireMetalView`,
//! `WMTViewSyncAndPoll`, `WMTPresentClientSurface`, and `MTLDrawable_setPresentedEvent`, plus
//! idempotent detachment for slot 73 (`ReleaseMetalView`).

use core::{
    ffi::{CStr, c_void},
    mem::transmute,
    ptr::{self, NonNull},
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
};
use std::sync::{Arc, LazyLock, Mutex, Once, PoisonError};

use block2::{Block, RcBlock};
use objc2::{
    MainThreadMarker, MainThreadOnly, extern_class, extern_methods,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, ProtocolObject},
};
use objc2_app_kit::{
    NSApplication, NSApplicationDidChangeScreenParametersNotification, NSScreen, NSView, NSWindow,
    NSWindowAnimationBehavior, NSWindowDidChangeOcclusionStateNotification, NSWindowOcclusionState,
    NSWindowOrderingMode,
};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_core_graphics::{
    CGColor, CGColorSpace, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
    kCGColorSpaceExtendedLinearDisplayP3, kCGColorSpaceExtendedLinearSRGB, kCGColorSpaceSRGB,
};
use objc2_foundation::{
    NSActivityOptions, NSNotification, NSNotificationCenter, NSNumber, NSProcessInfo, NSRect,
    NSString, NSThread, ns_string,
};
use objc2_metal::{MTLDevice, MTLDrawable, MTLPixelFormat, MTLSharedEvent};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer, CATransaction, kCAGravityResizeAspect};
use winemetal_wire::{
    UnixcallGenericObjRet, UnixcallGenericObjUint64Noret, UnixcallMtldrawableSetpresentedevent,
    UnixcallWmtAttachMetalView, UnixcallWmtViewSyncAndPoll, WMT_PRESENTED_EVENT_FLAG_PACED,
    WMT_RETIRE_METAL_VIEW_FLAG_PARK, WMT_VIEW_ATTACH_FLAG_COLOR_SPACE_ACCURATE,
    WMT_VIEW_ATTACH_FLAG_HDR_ENABLE, WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_OFF,
    WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON, WMT_VIEW_SYNC_FLAG_FORCE_REFRESH, WMTViewAttachDesc,
    WMTViewPollOut,
};

use super::{
    attachment::{
        self, AttachLatches, Attachment, CURSOR_POLICY_AUTO, CURSOR_POLICY_OFF, CURSOR_POLICY_ON,
        STATIC_ACCURATE_COLORSPACE, STATIC_HDR_ENABLE,
    },
    client_surface, cursor_overlay, delegate_forward,
};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_mut, mem_ref, params},
    float_cast::{f64_to_f32, f64_to_u32_saturating, u64_to_usize},
    handle::{obj, ret_owned},
};

const DRAWABLE_POOL_DEPTH: usize = 3;
const KEPT_METAL_VIEWS: usize = 4;

type GetWinDataFn = unsafe extern "C-unwind" fn(*mut c_void) -> *mut MacdrvWinData;
type ReleaseWinDataFn = unsafe extern "C-unwind" fn(*mut MacdrvWinData);
type CreateMetalViewFn = unsafe extern "C-unwind" fn(*mut c_void, *mut c_void) -> *mut c_void;
type GetMetalLayerFn = unsafe extern "C-unwind" fn(*mut c_void) -> *mut c_void;
type ReleaseMetalViewFn = unsafe extern "C-unwind" fn(*mut c_void);
type GetCocoaWindowFn = unsafe extern "C-unwind" fn(*mut c_void, i32) -> *mut c_void;

#[repr(C)]
struct MacdrvWinData {
    _hwnd: *mut c_void,
    _cocoa_window: *mut c_void,
    _cocoa_view: *mut c_void,
    client_cocoa_view: *mut c_void,
}

#[repr(C)]
struct MacdrvFunctionsTable {
    macdrv_init_display_devices: *mut c_void,
    get_win_data: *mut c_void,
    release_win_data: *mut c_void,
    macdrv_get_cocoa_window: *mut c_void,
    _macdrv_create_metal_device: *mut c_void,
    _macdrv_release_metal_device: *mut c_void,
    macdrv_view_create_metal_view: *mut c_void,
    macdrv_view_get_metal_layer: *mut c_void,
    macdrv_view_release_metal_view: *mut c_void,
    on_main_thread: *mut c_void,
}

struct MacdrvFuncs {
    get_win_data: GetWinDataFn,
    release_win_data: ReleaseWinDataFn,
    create_metal_view: CreateMetalViewFn,
    get_metal_layer: GetMetalLayerFn,
    release_metal_view: ReleaseMetalViewFn,
    get_cocoa_window: Option<GetCocoaWindowFn>,
}

fn symbol(name: &CStr) -> *mut c_void {
    // SAFETY: NUL-terminated static symbol name passed to `dlsym`.
    unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) }
}

fn macdrv_table() -> Option<&'static MacdrvFunctionsTable> {
    let sym = symbol(c"macdrv_functions");
    if sym.is_null() {
        return None;
    }
    // SAFETY: `macdrv_functions` is a process-lifetime table published by winemac.
    unsafe { sym.cast::<MacdrvFunctionsTable>().as_ref() }
}

impl MacdrvFuncs {
    fn load() -> Option<Self> {
        if let Some(t) = macdrv_table() {
            if t.get_win_data.is_null()
                || t.release_win_data.is_null()
                || t.macdrv_view_create_metal_view.is_null()
                || t.macdrv_view_get_metal_layer.is_null()
                || t.macdrv_view_release_metal_view.is_null()
            {
                return None;
            }
            // SAFETY: non-null winemac function pointer matching `GetWinDataFn`.
            let get_win_data = unsafe { transmute::<*mut c_void, GetWinDataFn>(t.get_win_data) };
            // SAFETY: non-null winemac function pointer matching `ReleaseWinDataFn`.
            let release_win_data =
                unsafe { transmute::<*mut c_void, ReleaseWinDataFn>(t.release_win_data) };
            // SAFETY: non-null winemac function pointer matching `CreateMetalViewFn`.
            let create_metal_view = unsafe {
                transmute::<*mut c_void, CreateMetalViewFn>(t.macdrv_view_create_metal_view)
            };
            // SAFETY: non-null winemac function pointer matching `GetMetalLayerFn`.
            let get_metal_layer =
                unsafe { transmute::<*mut c_void, GetMetalLayerFn>(t.macdrv_view_get_metal_layer) };
            // SAFETY: non-null winemac function pointer matching `ReleaseMetalViewFn`.
            let release_metal_view = unsafe {
                transmute::<*mut c_void, ReleaseMetalViewFn>(t.macdrv_view_release_metal_view)
            };
            // SAFETY: nullable winemac function pointer matching `Option<GetCocoaWindowFn>`.
            let get_cocoa_window = unsafe {
                transmute::<*mut c_void, Option<GetCocoaWindowFn>>(t.macdrv_get_cocoa_window)
            };
            return Some(Self {
                get_win_data,
                release_win_data,
                create_metal_view,
                get_metal_layer,
                release_metal_view,
                get_cocoa_window,
            });
        }
        let get = symbol(c"get_win_data");
        let rel = symbol(c"release_win_data");
        let create = symbol(c"macdrv_view_create_metal_view");
        let layer = symbol(c"macdrv_view_get_metal_layer");
        let release = symbol(c"macdrv_view_release_metal_view");
        if get.is_null()
            || rel.is_null()
            || create.is_null()
            || layer.is_null()
            || release.is_null()
        {
            return None;
        }
        // SAFETY: loose `get_win_data` export matching `GetWinDataFn`.
        let get_win_data = unsafe { transmute::<*mut c_void, GetWinDataFn>(get) };
        // SAFETY: loose `release_win_data` export matching `ReleaseWinDataFn`.
        let release_win_data = unsafe { transmute::<*mut c_void, ReleaseWinDataFn>(rel) };
        // SAFETY: loose `macdrv_view_create_metal_view` export matching `CreateMetalViewFn`.
        let create_metal_view = unsafe { transmute::<*mut c_void, CreateMetalViewFn>(create) };
        // SAFETY: loose `macdrv_view_get_metal_layer` export matching `GetMetalLayerFn`.
        let get_metal_layer = unsafe { transmute::<*mut c_void, GetMetalLayerFn>(layer) };
        // SAFETY: loose `macdrv_view_release_metal_view` export matching `ReleaseMetalViewFn`.
        let release_metal_view = unsafe { transmute::<*mut c_void, ReleaseMetalViewFn>(release) };
        // SAFETY: optional loose `macdrv_get_cocoa_window` export.
        let get_cocoa_window = unsafe {
            transmute::<*mut c_void, Option<GetCocoaWindowFn>>(symbol(c"macdrv_get_cocoa_window"))
        };
        Some(Self {
            get_win_data,
            release_win_data,
            create_metal_view,
            get_metal_layer,
            release_metal_view,
            get_cocoa_window,
        })
    }
}

fn has_cocoa_window() -> bool {
    if let Some(t) = macdrv_table() {
        return !t.macdrv_get_cocoa_window.is_null();
    }
    !symbol(c"macdrv_get_cocoa_window").is_null()
}

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_sync_f(queue: *mut c_void, ctx: *mut c_void, work: extern "C" fn(*mut c_void));
    fn dispatch_async_f(queue: *mut c_void, ctx: *mut c_void, work: extern "C" fn(*mut c_void));
}

fn wine_on_main_thread() -> Option<unsafe extern "C" fn(&Block<dyn Fn()>)> {
    let raw = macdrv_table().map_or_else(|| symbol(c"on_main_thread"), |t| t.on_main_thread);
    if raw.is_null() {
        return None;
    }
    // SAFETY: Wine's `on_main_thread` takes a `dispatch_block_t` (`&Block<dyn Fn()>`).
    Some(unsafe { transmute::<*mut c_void, unsafe extern "C" fn(&Block<dyn Fn()>)>(raw) })
}

fn run_through_wine(
    on_main_thread: unsafe extern "C" fn(&Block<dyn Fn()>),
    work: extern "C" fn(*mut c_void),
    ctx: *mut c_void,
) -> bool {
    const PENDING: u8 = 0;
    const RUNNING: u8 = 1;
    const DONE: u8 = 2;
    const ABANDONED: u8 = 3;
    let state = Arc::new(AtomicU8::new(PENDING));
    let block_state = Arc::clone(&state);
    let ctx_addr = ctx.expose_provenance();
    let block = RcBlock::new(move || {
        autoreleasepool(|_| {
            let _guard = AbortOnPanic;
            if block_state
                .compare_exchange(PENDING, RUNNING, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                work(ptr::with_exposed_provenance_mut(ctx_addr));
                block_state.store(DONE, Ordering::Release);
            }
        });
    });
    // SAFETY: `on_main_thread` executes or drops `block` synchronously while `ctx` stays live.
    unsafe { on_main_thread(&block) };
    loop {
        match state.compare_exchange(PENDING, ABANDONED, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return false,
            Err(DONE) => return true,
            Err(_) => std::thread::yield_now(),
        }
    }
}

/// Run `f` synchronously on `AppKit`'s main thread, checking `NSThread::isMainThread` first.
///
/// # Panics
///
/// Panics if the main-thread dispatch completes without executing `f`.
pub fn run_on_main_thread_sync<F: FnOnce() -> R, R>(f: F) -> R {
    struct CallCtx<F, R> {
        f: Option<F>,
        out: Option<R>,
    }
    extern "C" fn thunk<F: FnOnce() -> R, R>(ctx: *mut c_void) {
        // SAFETY: `ctx` is `&mut CallCtx<F, R>` on the waiting caller's stack.
        let ctx = unsafe { &mut *ctx.cast::<CallCtx<F, R>>() };
        if let Some(f) = ctx.f.take() {
            ctx.out = Some(autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                f()
            }));
        }
    }
    static WINE_HOP_DROPS_WORK: AtomicBool = AtomicBool::new(false);
    if NSThread::isMainThread_class() {
        return autoreleasepool(|_| {
            let _guard = AbortOnPanic;
            f()
        });
    }
    let mut ctx = CallCtx {
        f: Some(f),
        out: None,
    };
    let ctx_ptr = (&raw mut ctx).cast::<c_void>();
    if !WINE_HOP_DROPS_WORK.load(Ordering::Relaxed)
        && let Some(on_main_thread) = wine_on_main_thread()
    {
        if run_through_wine(on_main_thread, thunk::<F, R>, ctx_ptr) {
            return ctx
                .out
                .expect("run_on_main_thread_sync closure produced no result");
        }
        WINE_HOP_DROPS_WORK.store(true, Ordering::Relaxed);
    }
    let main_q = (&raw const _dispatch_main_q).cast_mut().cast::<c_void>();
    // SAFETY: `dispatch_sync_f` runs `thunk` to completion before returning while `ctx` is live.
    unsafe { dispatch_sync_f(main_q, ctx_ptr, thunk::<F, R>) };
    ctx.out
        .expect("run_on_main_thread_sync closure produced no result")
}

/// Run `f` asynchronously on libdispatch's main queue inside an autorelease pool.
pub fn run_on_main_thread_async<F: FnOnce() + Send + 'static>(f: F) {
    extern "C" fn thunk<F: FnOnce()>(ctx: *mut c_void) {
        // SAFETY: `ctx` came from `Box::into_raw` below and runs once.
        let f = unsafe { Box::from_raw(ctx.cast::<F>()) };
        autoreleasepool(|_| {
            let _guard = AbortOnPanic;
            f();
        });
    }
    let ctx = Box::into_raw(Box::new(f));
    let main_q = (&raw const _dispatch_main_q).cast_mut().cast::<c_void>();
    // SAFETY: `_dispatch_main_q` is libSystem's main queue and `ctx` transfers to `thunk`.
    unsafe { dispatch_async_f(main_q, ctx.cast::<c_void>(), thunk::<F>) };
}

static LATENCY_CRITICAL_ACTIVE: LazyLock<bool> = LazyLock::new(|| {
    autoreleasepool(|_| {
        let options =
            NSActivityOptions::UserInteractive | NSActivityOptions::IdleDisplaySleepDisabled;
        let reason = NSString::from_str("winemetal: continuous latency-critical rendering");
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &reason);
        core::mem::forget(token);
        true
    })
});

/// Slot 158, `_WMTDeclareLatencyCriticalActivity`: declares `NSActivityUserInteractive` once.
///
/// # Safety
///
/// `args` is slot 158's `unixcall_generic_obj_ret`.
#[unsafe(export_name = "_WMTDeclareLatencyCriticalActivity")]
pub unsafe extern "C-unwind" fn wmt_declare_latency_critical_activity(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 158 carries `UnixcallGenericObjRet`.
        let p = unsafe { params::<UnixcallGenericObjRet>(args) };
        p.ret = u64::from(*LATENCY_CRITICAL_ACTIVE);
        STATUS_SUCCESS
    })
}

extern_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "WineWindow"]
    struct WineWindow;
);

impl WineWindow {
    extern_methods!(
        #[unsafe(method(windowDidDrawContent))]
        #[unsafe(method_family = none)]
        fn window_did_draw_content(&self);

        #[unsafe(method(closing))]
        #[unsafe(method_family = none)]
        fn closing(&self) -> bool;
    );
}

static HAS_WINE_WINDOW: LazyLock<bool> = LazyLock::new(|| AnyClass::get(c"WineWindow").is_some());

struct KeptMetalView {
    hwnd: u64,
    view: usize,
    layer: usize,
    seq: u64,
}

impl KeptMetalView {
    const EMPTY: Self = Self {
        hwnd: 0,
        view: 0,
        layer: 0,
        seq: 0,
    };
}

struct MetalViewPark {
    slots: [KeptMetalView; KEPT_METAL_VIEWS],
    next_seq: u64,
}

impl MetalViewPark {
    const fn new() -> Self {
        Self {
            slots: [KeptMetalView::EMPTY; KEPT_METAL_VIEWS],
            next_seq: 1,
        }
    }

    fn park(&mut self, hwnd: u64, view: usize, layer: usize) -> Option<usize> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let kept = KeptMetalView {
            hwnd,
            view,
            layer,
            seq,
        };
        let slot = self
            .slots
            .iter()
            .position(|s| s.view != 0 && s.hwnd == hwnd)
            .or_else(|| self.slots.iter().position(|s| s.view == 0))
            .or_else(|| {
                self.slots
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, s)| s.seq)
                    .map(|(idx, _)| idx)
            })
            .expect("park has at least one slot");
        let displaced = self.slots[slot].view;
        self.slots[slot] = kept;
        (displaced != 0 && displaced != view).then_some(displaced)
    }

    fn take_for(&mut self, hwnd: u64) -> Option<(usize, usize)> {
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.view != 0 && s.hwnd == hwnd)?;
        let out = (slot.view, slot.layer);
        *slot = KeptMetalView::EMPTY;
        Some(out)
    }

    fn slots_newest_first(&self) -> Vec<(u64, usize, usize, u64)> {
        let mut active: Vec<_> = self
            .slots
            .iter()
            .filter(|s| s.view != 0)
            .map(|s| (s.hwnd, s.view, s.layer, s.seq))
            .collect();
        active.sort_by_key(|a| std::cmp::Reverse(a.3));
        active
    }

    fn take_kept(&mut self, view: usize, seq: u64) -> Option<KeptMetalView> {
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.view == view && s.seq == seq)?;
        let taken = KeptMetalView {
            hwnd: slot.hwnd,
            view: slot.view,
            layer: slot.layer,
            seq: slot.seq,
        };
        *slot = KeptMetalView::EMPTY;
        Some(taken)
    }
}

static PARKED_METAL_VIEW: Mutex<MetalViewPark> = Mutex::new(MetalViewPark::new());

fn release_raw_metal_view(release: ReleaseMetalViewFn, view: usize) {
    if view != 0 {
        // SAFETY: `view` is a retained `WineMetalView*` owned by the caller or displaced from park.
        unsafe { release(view as *mut c_void) };
    }
}

fn retain_parked_view(view: usize, _mtm: MainThreadMarker) -> Option<Retained<NSView>> {
    // SAFETY: caller popped or holds the park lock while `view` is still retained by the park.
    unsafe { Retained::retain(view as *mut NSView) }
}

fn kept_view_host_gone(view: usize, seq: u64) -> bool {
    let mut gone = false;
    run_on_main_thread_sync(|| {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let retained = {
            let park = PARKED_METAL_VIEW
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            park.slots
                .iter()
                .find(|s| s.view == view && s.seq == seq)
                .and_then(|s| retain_parked_view(s.view, mtm))
        };
        let Some(v) = retained else {
            return;
        };
        let Some(window) = v.window() else {
            gone = true;
            return;
        };
        if *HAS_WINE_WINDOW && let Some(wine_win) = window.downcast_ref::<WineWindow>() {
            gone = wine_win.closing();
        }
    });
    gone
}

enum KeptView {
    Hosted { view: usize, layer: usize },
    Orphan { view: usize, layer: usize },
}

fn kept_metal_view(funcs: &MacdrvFuncs, hwnd: u64) -> Option<KeptView> {
    let get_cocoa_window = funcs.get_cocoa_window?;
    let own = PARKED_METAL_VIEW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take_for(hwnd);
    if let Some((view, layer)) = own {
        // SAFETY: Wine `macdrv_get_cocoa_window` takes HWND and `create=0`.
        let cocoa_win = unsafe { get_cocoa_window(hwnd as *mut c_void, 0) } as usize;
        let mut hosted = false;
        if cocoa_win != 0 {
            run_on_main_thread_sync(|| {
                if let Some(mtm) = MainThreadMarker::new() {
                    hosted = retain_parked_view(view, mtm)
                        .and_then(|v| v.window())
                        .is_some_and(|w| Retained::as_ptr(&w) as usize == cocoa_win);
                }
            });
        }
        return Some(if hosted {
            KeptView::Hosted { view, layer }
        } else {
            KeptView::Orphan { view, layer }
        });
    }
    let newest_first = PARKED_METAL_VIEW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .slots_newest_first();
    let orphan = newest_first
        .into_iter()
        .find(|&(kept_hwnd, view, _, seq)| {
            // SAFETY: Wine `macdrv_get_cocoa_window` takes HWND and `create=0`.
            let win = unsafe { get_cocoa_window(kept_hwnd as *mut c_void, 0) };
            win.is_null() && kept_view_host_gone(view, seq)
        })?;
    let taken = PARKED_METAL_VIEW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take_kept(orphan.1, orphan.3)?;
    Some(KeptView::Orphan {
        view: taken.view,
        layer: taken.layer,
    })
}

fn retain_client_surface(
    client_view: *mut c_void,
    calls: Option<&client_surface::SurfaceCalls>,
) -> usize {
    let Some(calls) = calls else {
        return 0;
    };
    if client_view.is_null() {
        return 0;
    }
    let addr = client_view as usize;
    let surface = run_on_main_thread_sync(|| {
        MainThreadMarker::new().map_or(0, |mtm| {
            client_surface::client_view_surface(addr as *mut c_void, mtm)
        })
    });
    calls.retain(surface)
}

fn create_new_view(
    funcs: &MacdrvFuncs,
    hwnd: u64,
    device_handle: u64,
    calls: Option<&client_surface::SurfaceCalls>,
) -> Option<(usize, usize, usize)> {
    // SAFETY: winemac `get_win_data` returns null or a locked `MacdrvWinData`.
    let win_data = unsafe { (funcs.get_win_data)(hwnd as *mut c_void) };
    if win_data.is_null() {
        return None;
    }
    // SAFETY: `win_data` stays locked until `release_win_data`.
    let client = unsafe { (*win_data).client_cocoa_view };
    let out = if client.is_null() {
        None
    } else {
        // SAFETY: `client` is valid while `win_data` is held; `device_handle` is live or 0.
        let view = unsafe { (funcs.create_metal_view)(client, device_handle as *mut c_void) };
        if view.is_null() {
            None
        } else {
            // SAFETY: `view` was just returned non-null by `create_metal_view`.
            let layer = unsafe { (funcs.get_metal_layer)(view) };
            if layer.is_null() {
                release_raw_metal_view(funcs.release_metal_view, view as usize);
                None
            } else {
                let surface = retain_client_surface(client, calls);
                Some((view as usize, layer as usize, surface))
            }
        }
    };
    // SAFETY: balances the non-null `get_win_data` above.
    unsafe { (funcs.release_win_data)(win_data) };
    out
}

fn adopt_kept_view(
    funcs: &MacdrvFuncs,
    hwnd: u64,
    device_handle: u64,
    kept: (usize, usize),
    calls: Option<&client_surface::SurfaceCalls>,
) -> Option<(usize, usize, usize)> {
    let (view, layer) = kept;
    // SAFETY: winemac `get_win_data` returns null or a locked `MacdrvWinData`.
    let win_data = unsafe { (funcs.get_win_data)(hwnd as *mut c_void) };
    if win_data.is_null() {
        release_raw_metal_view(funcs.release_metal_view, view);
        return None;
    }
    // SAFETY: `win_data` stays locked until `release_win_data`.
    let client = unsafe { (*win_data).client_cocoa_view };
    // SAFETY: `view` was retained by the park and popped for this attach.
    let carried = unsafe { (funcs.get_metal_layer)(view as *mut c_void) } as usize;
    let moved = if !client.is_null() && carried == layer {
        let client_addr = client as usize;
        run_on_main_thread_sync(|| {
            let Some(mtm) = MainThreadMarker::new() else {
                return false;
            };
            let Some(v) = retain_parked_view(view, mtm) else {
                return false;
            };
            // SAFETY: `client_addr` is retained by the locked `WinData` for this synchronous hop.
            let Some(c) = (unsafe { Retained::retain(client_addr as *mut NSView) }) else {
                return false;
            };
            v.removeFromSuperview();
            v.setFrame(c.bounds());
            c.setAutoresizesSubviews(true);
            c.addSubview_positioned_relativeTo(&v, NSWindowOrderingMode::Below, None);
            if *HAS_WINE_WINDOW
                && let Some(win) = c.window()
                && let Some(wine_win) = win.downcast_ref::<WineWindow>()
            {
                wine_win.window_did_draw_content();
            }
            true
        })
    } else {
        false
    };
    let out = if moved {
        Some((view, layer, retain_client_surface(client, calls)))
    } else {
        release_raw_metal_view(funcs.release_metal_view, view);
        if client.is_null() {
            None
        } else {
            // SAFETY: `client` is valid under `win_data`.
            let fresh_view =
                unsafe { (funcs.create_metal_view)(client, device_handle as *mut c_void) };
            if fresh_view.is_null() {
                None
            } else {
                // SAFETY: `fresh_view` is non-null.
                let fresh_layer = unsafe { (funcs.get_metal_layer)(fresh_view) };
                if fresh_layer.is_null() {
                    release_raw_metal_view(funcs.release_metal_view, fresh_view as usize);
                    None
                } else {
                    Some((
                        fresh_view as usize,
                        fresh_layer as usize,
                        retain_client_surface(client, calls),
                    ))
                }
            }
        }
    };
    // SAFETY: balances the non-null `get_win_data` above.
    unsafe { (funcs.release_win_data)(win_data) };
    out
}

fn retain_hosted_surface(view: usize, calls: Option<&client_surface::SurfaceCalls>) -> usize {
    let Some(calls) = calls else {
        return 0;
    };
    let surface = run_on_main_thread_sync(|| {
        let Some(mtm) = MainThreadMarker::new() else {
            return 0;
        };
        let Some(v) = retain_parked_view(view, mtm) else {
            return 0;
        };
        // SAFETY: main-thread superview access while `v` is retained.
        let Some(client) = (unsafe { v.superview() }) else {
            return 0;
        };
        client_surface::client_view_surface(Retained::as_ptr(&client).cast_mut().cast(), mtm)
    });
    calls.retain(surface)
}

fn screen_number(screen: &NSScreen) -> u32 {
    let desc = screen.deviceDescription();
    desc.objectForKey(ns_string!("NSScreenNumber"))
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map_or(0, |n| n.unsignedIntValue())
}

fn screen_max_hz(screen: &NSScreen) -> u32 {
    let clamped = screen.maximumFramesPerSecond().clamp(0, 1000);
    u32::try_from(clamped).unwrap_or(0)
}

fn layer_hdr_active(potential: f64, hdr_enable: bool) -> bool {
    hdr_enable && potential > 1.0 && potential.is_finite()
}

fn apply_layer_color(layer: &CAMetalLayer, screen: Option<&NSScreen>, hdr: bool, accurate: bool) {
    layer.setPixelFormat(if hdr {
        MTLPixelFormat::RGBA16Float
    } else {
        MTLPixelFormat::BGRA8Unorm
    });
    let native_cs = screen
        .and_then(NSScreen::colorSpace)
        .and_then(|ns| ns.CGColorSpace());
    match (hdr, accurate) {
        (true, true) => {
            // SAFETY: process-lifetime CoreGraphics constant.
            let name = unsafe { kCGColorSpaceExtendedLinearSRGB };
            if let Some(cs) = CGColorSpace::with_name(Some(name)) {
                layer.setColorspace(Some(&cs));
            }
        }
        (true, false) => {
            if let Some(hdr_cs) = native_cs
                .as_deref()
                .and_then(CGColorSpace::extended_linearized)
            {
                layer.setColorspace(Some(&hdr_cs));
            } else {
                // SAFETY: process-lifetime CoreGraphics constant.
                let name = unsafe { kCGColorSpaceExtendedLinearDisplayP3 };
                if let Some(cs) = CGColorSpace::with_name(Some(name)) {
                    layer.setColorspace(Some(&cs));
                }
            }
        }
        (false, true) => {
            // SAFETY: process-lifetime CoreGraphics constant.
            let name = unsafe { kCGColorSpaceSRGB };
            if let Some(cs) = CGColorSpace::with_name(Some(name)) {
                layer.setColorspace(Some(&cs));
            }
        }
        (false, false) => {
            if let Some(cs) = native_cs.as_deref() {
                let sdr = cs.copy_with_standard_range();
                layer.setColorspace(Some(&sdr));
            } else {
                // SAFETY: process-lifetime CoreGraphics constant.
                let name = unsafe { kCGColorSpaceSRGB };
                if let Some(cs) = CGColorSpace::with_name(Some(name)) {
                    layer.setColorspace(Some(&cs));
                }
            }
        }
    }
    layer.setWantsExtendedDynamicRangeContent(hdr);
}

fn natural_drawable_size(layer: &CAMetalLayer) -> (u32, u32) {
    let bounds = layer.bounds();
    let scale = layer.contentsScale();
    (
        f64_to_u32_saturating((bounds.size.width * scale).round()),
        f64_to_u32_saturating((bounds.size.height * scale).round()),
    )
}

fn sync_drawable_size(layer: &CAMetalLayer) -> (u32, u32) {
    let (native_w, native_h) = natural_drawable_size(layer);
    if native_w == 0 || native_h == 0 {
        return (native_w, native_h);
    }
    let current = layer.drawableSize();
    let width = f64::from(native_w);
    let height = f64::from(native_h);
    if (current.width - width).abs() <= 0.0 && (current.height - height).abs() <= 0.0 {
        return (native_w, native_h);
    }
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    layer.setDrawableSize(CGSize { width, height });
    CATransaction::commit();
    (native_w, native_h)
}

fn rect_covers(inner: CGRect, outer: CGRect) -> bool {
    const SLACK: f64 = 1.0 / 64.0;
    let imin = inner.min();
    let imax = inner.max();
    let omin = outer.min();
    let omax = outer.max();
    imin.x <= omin.x + SLACK
        && imin.y <= omin.y + SLACK
        && imax.x >= omax.x - SLACK
        && imax.y >= omax.y - SLACK
}

fn clear_covered_window_layers(view: &NSView) {
    let Some(window) = view.window() else {
        return;
    };
    if view.isHiddenOrHasHiddenAncestor() {
        return;
    }
    let content = window.contentView();
    let frame_view = content.as_deref().and_then(|c| {
        // SAFETY: main-thread view hierarchy traversal on retained `content`.
        unsafe { c.superview() }
    });
    let (Some(content), Some(frame_view)) = (content, frame_view) else {
        return;
    };
    if !rect_covers(
        view.convertRect_toView(view.visibleRect(), None),
        frame_view.convertRect_toView(frame_view.bounds(), None),
    ) {
        return;
    }
    let (Some(content_layer), Some(frame_layer)) = (content.layer(), frame_view.layer()) else {
        return;
    };
    if frame_layer.backgroundColor().is_some() {
        frame_layer.setBackgroundColor(None);
    }
    // SAFETY: checking `CALayer.contents` presence on main thread.
    if unsafe { content_layer.contents() }.is_some() {
        // SAFETY: `None` clears the window backing surface beneath the opaque Metal view.
        unsafe { content_layer.setContents(None) };
    }
}

fn install_occlusion_observer_once() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
            autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                // SAFETY: AppKit passes a live `NSNotification`.
                let notification = unsafe { notification.as_ref() };
                let Some(object) = notification.object() else {
                    return;
                };
                let Some(window) = object.downcast_ref::<NSWindow>() else {
                    return;
                };
                let window_ptr = core::ptr::from_ref(window) as usize;
                let ours: Vec<Arc<Attachment>> = attachment::live()
                    .into_iter()
                    .filter(|att| window_ptr != 0 && att.window() == window_ptr)
                    .collect();
                if ours.is_empty() {
                    return;
                }
                let occluded = !window
                    .occlusionState()
                    .contains(NSWindowOcclusionState::Visible);
                for att in &ours {
                    att.set_window_occluded(occluded);
                }
            });
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: process-lifetime AppKit notification constant.
        let name = unsafe { NSWindowDidChangeOcclusionStateNotification };
        // SAFETY: center copies the block; token is intentionally kept for process lifetime.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
        };
        core::mem::forget(token);
    });
}

#[derive(PartialEq, Eq)]
struct ScreenEntry {
    frame: [u64; 4],
    visible_frame: [u64; 4],
    scale: u64,
}

#[derive(PartialEq, Eq)]
struct ScreenConfiguration {
    screens: Vec<ScreenEntry>,
    main_mode: (usize, usize, u64, i32),
}

fn current_screen_configuration(mtm: MainThreadMarker) -> ScreenConfiguration {
    let rect_bits = |r: NSRect| {
        [
            r.origin.x.to_bits(),
            r.origin.y.to_bits(),
            r.size.width.to_bits(),
            r.size.height.to_bits(),
        ]
    };
    let screens = NSScreen::screens(mtm)
        .iter()
        .map(|s| ScreenEntry {
            frame: rect_bits(s.frame()),
            visible_frame: rect_bits(s.visibleFrame()),
            scale: s.backingScaleFactor().to_bits(),
        })
        .collect();
    let main_mode = CGDisplayCopyDisplayMode(CGMainDisplayID()).map_or((0, 0, 0, 0), |m| {
        (
            CGDisplayMode::width(Some(&m)),
            CGDisplayMode::height(Some(&m)),
            CGDisplayMode::refresh_rate(Some(&m)).to_bits(),
            CGDisplayMode::io_display_mode_id(Some(&m)),
        )
    });
    ScreenConfiguration { screens, main_mode }
}

static SCREEN_PARAMS_FILTER_INSTALLED: AtomicBool = AtomicBool::new(false);
static WINE_APP_DELEGATE_PTR: AtomicUsize = AtomicUsize::new(0);
static LAST_SCREEN_CONFIG: Mutex<Option<ScreenConfiguration>> = Mutex::new(None);

fn install_screen_params_filter(mtm: MainThreadMarker) {
    if !has_cocoa_window() || SCREEN_PARAMS_FILTER_INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
        return;
    };
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: process-lifetime AppKit notification constant.
    let name = unsafe { NSApplicationDidChangeScreenParametersNotification };
    // SAFETY: `ProtocolObject` wraps `AnyObject` transparently.
    let observer = unsafe { &*Retained::as_ptr(&delegate).cast::<AnyObject>() };
    if delegate_forward::delegate_handles_screen_parameters(observer) {
        // SAFETY: unregistering Wine's delegate for this notification on main thread.
        unsafe { center.removeObserver_name_object(observer, Some(name), None) };
        WINE_APP_DELEGATE_PTR.store(Retained::as_ptr(&delegate) as usize, Ordering::Release);
        core::mem::forget(delegate);
    }
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        autoreleasepool(|_| {
            let _guard = AbortOnPanic;
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let config = current_screen_configuration(mtm);
            let changed = {
                let mut last = LAST_SCREEN_CONFIG
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let changed = last.as_ref() != Some(&config);
                if changed {
                    *last = Some(config);
                }
                changed
            };
            if !changed {
                return;
            }
            for att in attachment::live() {
                refresh_attachment_on_main(&att);
            }
            let delegate_ptr = WINE_APP_DELEGATE_PTR.load(Ordering::Acquire);
            if delegate_ptr == 0 {
                return;
            }
            // SAFETY: delegate pointer was leaked above and lives on main thread.
            let delegate = unsafe { &*(delegate_ptr as *const AnyObject) };
            // SAFETY: notification is live for this callback.
            let notification = unsafe { notification.as_ref() };
            // SAFETY: called on main thread for Wine's delegate.
            let _ = unsafe { delegate_forward::forward_screen_parameters(delegate, notification) };
        });
    });
    // SAFETY: notification center copies the block; token is kept for process lifetime.
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
    };
    core::mem::forget(token);
    SCREEN_PARAMS_FILTER_INSTALLED.store(true, Ordering::Relaxed);
}

fn sanitize_edr(value: f64) -> f32 {
    if value.is_finite() && value >= 1.0 {
        f64_to_f32(value)
    } else {
        1.0
    }
}

fn refresh_attachment_on_main(att: &Arc<Attachment>) {
    att.end_refresh();
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let cocoa_ok = has_cocoa_window();
    let view_obj = if cocoa_ok {
        let Some(v) = attachment::retain_view(att, mtm) else {
            return;
        };
        Some(v)
    } else {
        if attachment::find(att.view()).is_none_or(|cur| !Arc::ptr_eq(&cur, att)) {
            return;
        }
        None
    };
    install_screen_params_filter(mtm);
    let win = view_obj.as_deref().and_then(NSView::window);
    if cocoa_ok {
        if let Some(w) = win.as_deref() {
            att.set_window(ptr::from_ref::<NSWindow>(w) as usize);
            let occluded = !w.occlusionState().contains(NSWindowOcclusionState::Visible);
            att.set_window_occluded(occluded);
        } else {
            att.set_window(0);
            att.set_window_occluded(false);
        }
    }
    let screen = win
        .as_deref()
        .and_then(NSWindow::screen)
        .or_else(|| NSScreen::mainScreen(mtm));
    let headroom = screen.as_deref().map_or(
        1.0,
        NSScreen::maximumExtendedDynamicRangeColorComponentValue,
    );
    att.set_headroom(sanitize_edr(headroom));
    if let Some(screen) = screen.as_deref() {
        let potential = screen.maximumPotentialExtendedDynamicRangeColorComponentValue();
        att.set_edr_potential(sanitize_edr(potential));
        att.set_panel_max_hz(screen_max_hz(screen));
        let screen_id = screen_number(screen);
        let prev_screen_id = att.swap_screen_id(screen_id);
        let screen_moved = prev_screen_id != 0 && screen_id != 0 && prev_screen_id != screen_id;
        let want_hdr = layer_hdr_active(potential, att.hdr_enable_requested());
        if (want_hdr != att.hdr_active() || (!att.accurate_colorspace() && screen_moved))
            && let Some(layer) = attachment::retain_layer(att, mtm)
        {
            apply_layer_color(&layer, Some(screen), want_hdr, att.accurate_colorspace());
            att.set_hdr_active(want_hdr);
            att.mark_colorspace_changed();
        }
        if let Some(layer) = attachment::retain_layer(att, mtm) {
            let scale = f64_to_u32_saturating(layer.contentsScale().round()).clamp(1, 8);
            att.set_backing_scale(scale);
        }
        if let Some(v) = view_obj.as_deref() {
            clear_covered_window_layers(v);
        }
    }
    cursor_overlay::reconcile_on_main();
}

/// Slot 159, `_WMTAttachMetalView`: attaches or reuses a view and configures its `CAMetalLayer`.
///
/// # Safety
///
/// `args` is slot 159's `unixcall_wmt_attach_metal_view`.
#[unsafe(export_name = "_WMTAttachMetalView")]
pub unsafe extern "C-unwind" fn wmt_attach_metal_view(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 159 carries `UnixcallWmtAttachMetalView`.
        let p = unsafe { params::<UnixcallWmtAttachMetalView>(args) };
        p.ret_view = 0;
        p.ret_layer = 0;
        if p.hwnd == 0 || p.device == 0 {
            return STATUS_SUCCESS;
        }
        let Some(funcs) = MacdrvFuncs::load() else {
            return STATUS_SUCCESS;
        };
        // SAFETY: caller provides readable `WMTViewAttachDesc` or null.
        let attach_flags = unsafe { mem_ref::<WMTViewAttachDesc>(p.desc) }.map_or(0, |d| d.flags);
        let hdr_enable = attach_flags & WMT_VIEW_ATTACH_FLAG_HDR_ENABLE != 0;
        let accurate = attach_flags & WMT_VIEW_ATTACH_FLAG_COLOR_SPACE_ACCURATE != 0;
        let cursor_policy = if attach_flags & WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON != 0 {
            CURSOR_POLICY_ON
        } else if attach_flags & WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_OFF != 0 {
            CURSOR_POLICY_OFF
        } else {
            CURSOR_POLICY_AUTO
        };

        let surface_calls = client_surface::SurfaceCalls::load();
        let kept = kept_metal_view(&funcs, p.hwnd);
        let from_park = kept.is_some();
        let acquired = match kept {
            Some(KeptView::Hosted { view, layer }) => Some((
                view,
                layer,
                retain_hosted_surface(view, surface_calls.as_ref()),
            )),
            Some(KeptView::Orphan { view, layer }) => adopt_kept_view(
                &funcs,
                p.hwnd,
                p.device,
                (view, layer),
                surface_calls.as_ref(),
            ),
            None => create_new_view(&funcs, p.hwnd, p.device, surface_calls.as_ref()),
        };
        let Some((view, layer, client_surface)) = acquired else {
            return STATUS_SUCCESS;
        };

        let cocoa_ok = funcs.get_cocoa_window.is_some();
        let can_present = surface_calls.is_some();
        let device_addr = u64_to_usize(p.device);
        let mut backing_scale = 1u32;
        let mut edr_current = 1.0f32;
        let mut edr_potential = 1.0f32;
        let mut panel_max_hz = 0u32;
        let mut hdr_active = false;
        let mut dw = 0u32;
        let mut dh = 0u32;
        let mut screen_id = 0u32;

        run_on_main_thread_sync(|| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let view_obj = if cocoa_ok {
                // SAFETY: `view` was just acquired (+1) and is a live `NSView` under Cocoa.
                unsafe { Retained::retain(view as *mut NSView) }
            } else {
                None
            };
            if from_park
                && let Some(v) = view_obj.as_deref()
                && v.isHidden()
            {
                v.setHidden(false);
            }
            let win = view_obj.as_deref().and_then(NSView::window);
            if let Some(w) = win.as_deref()
                && w.animationBehavior() != NSWindowAnimationBehavior::None
            {
                w.setAnimationBehavior(NSWindowAnimationBehavior::None);
            }
            let screen = win
                .as_deref()
                .and_then(NSWindow::screen)
                .or_else(|| NSScreen::mainScreen(mtm));
            if let Some(s) = screen.as_deref() {
                let potential = s.maximumPotentialExtendedDynamicRangeColorComponentValue();
                let current = s.maximumExtendedDynamicRangeColorComponentValue();
                edr_potential = sanitize_edr(potential);
                edr_current = sanitize_edr(current);
                panel_max_hz = screen_max_hz(s);
                screen_id = screen_number(s);
                hdr_active = layer_hdr_active(potential, hdr_enable);
            }
            client_surface::bypass_present_hook(layer as *mut c_void, can_present, mtm);
            // SAFETY: `layer` is owned by `view` and stays alive for this call.
            let Some(layer_obj) = (unsafe { Retained::retain(layer as *mut CAMetalLayer) }) else {
                return;
            };
            backing_scale = f64_to_u32_saturating(layer_obj.contentsScale().round()).clamp(1, 8);
            // SAFETY: caller provides live `MTLDevice` handle for this attach.
            let dev = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(device_addr as u64) };
            layer_obj.setDevice(dev);
            apply_layer_color(&layer_obj, screen.as_deref(), hdr_active, accurate);
            layer_obj.setOpaque(true);
            // SAFETY: process-lifetime CoreAnimation constant.
            let gravity = unsafe { kCAGravityResizeAspect };
            layer_obj.setContentsGravity(gravity);
            let backdrop = CGColor::new_generic_gray(0.0, 1.0);
            layer_obj.setBackgroundColor(Some(&backdrop));
            layer_obj.setFramebufferOnly(true);
            layer_obj.setDisplaySyncEnabled(false);
            layer_obj.setMaximumDrawableCount(DRAWABLE_POOL_DEPTH);
            layer_obj.setAllowsNextDrawableTimeout(true);
            layer_obj.setPresentsWithTransaction(false);
            if cocoa_ok {
                (dw, dh) = sync_drawable_size(&layer_obj);
                if let Some(v) = view_obj.as_deref() {
                    clear_covered_window_layers(v);
                }
                install_screen_params_filter(mtm);
            }
            cursor_overlay::install_pointer_watch(mtm);
        });

        let static_flags = (if hdr_enable { STATIC_HDR_ENABLE } else { 0 })
            | (if accurate {
                STATIC_ACCURATE_COLORSPACE
            } else {
                0
            });
        let (att, stale) = attachment::register(
            view,
            layer,
            &AttachLatches {
                hwnd: p.hwnd,
                static_flags,
                hdr_active,
                software_cursor_policy: cursor_policy,
                backing_scale,
                edr_current,
                edr_potential,
                panel_max_hz,
                screen_id,
                client_surface,
            },
        );
        if let Some(stale) = stale {
            cleanup_detached_record(&stale);
        }
        if cocoa_ok {
            let att_post = Arc::clone(&att);
            run_on_main_thread_sync(move || {
                let Some(mtm) = MainThreadMarker::new() else {
                    return;
                };
                if let Some(v) = attachment::retain_view(&att_post, mtm) {
                    client_surface::watch(&att_post, &v, mtm);
                    if let Some(win) = v.window() {
                        att_post.set_window(Retained::as_ptr(&win) as usize);
                        let occluded = !win
                            .occlusionState()
                            .contains(NSWindowOcclusionState::Visible);
                        att_post.set_window_occluded(occluded);
                    } else {
                        att_post.set_window(0);
                        att_post.set_window_occluded(false);
                    }
                }
                install_occlusion_observer_once();
            });
        }
        if let Some(calls) = &surface_calls {
            calls.present(client_surface);
        }
        // SAFETY: caller provides writable `WMTViewPollOut` or null.
        if let Some(out) = unsafe { mem_mut::<WMTViewPollOut>(p.out_poll) } {
            att.write_poll_out(dw, dh, out);
        }
        p.ret_view = view as u64;
        // SAFETY: return a +1 reference on `CAMetalLayer` alongside `ret_view`.
        let layer_ret = unsafe { Retained::retain(layer as *mut CAMetalLayer) };
        p.ret_layer = ret_owned(layer_ret);
        STATUS_SUCCESS
    })
}

fn cleanup_detached_record(att: &Arc<Attachment>) {
    client_surface::unwatch(att);
    cursor_overlay::detach(att);
    if let Some(calls) = client_surface::SurfaceCalls::load() {
        if let Some(sibling) = attachment::retain_newest_surface_on(att.hwnd(), |s| calls.retain(s))
        {
            calls.present(sibling);
            calls.release(sibling);
        }
        calls.release(att.client_surface());
    }
}

/// Idempotently unregister and tear down any live attachment state for `view_handle`.
#[must_use]
pub fn detach_metal_view_if_attached(view_handle: u64) -> Option<Arc<Attachment>> {
    if view_handle == 0 {
        return None;
    }
    let att = attachment::unregister(u64_to_usize(view_handle))?;
    cleanup_detached_record(&att);
    Some(att)
}

fn prepare_parked_view(view: usize, layer: usize) {
    run_on_main_thread_sync(|| {
        let Some(_mtm) = MainThreadMarker::new() else {
            return;
        };
        // SAFETY: `layer` stays owned by the retiring view until parked.
        if let Some(l) = unsafe { Retained::retain(layer as *mut CAMetalLayer) } {
            l.setDrawableSize(CGSize {
                width: 1.0,
                height: 1.0,
            });
        }
        // SAFETY: `view` is still retained by the retiring caller.
        if let Some(v) = unsafe { Retained::retain(view as *mut NSView) } {
            if let Some(w) = v.window()
                && w.animationBehavior() != NSWindowAnimationBehavior::None
            {
                w.setAnimationBehavior(NSWindowAnimationBehavior::None);
            }
            v.setHidden(true);
            if let Some(content) = v.window().and_then(|w| w.contentView()) {
                content.setNeedsDisplay(true);
            }
        }
    });
}

/// Slot 160, `_WMTRetireMetalView`: detaches an attachment and optionally parks the metal view.
///
/// # Safety
///
/// `args` is slot 160's `unixcall_generic_obj_uint64_noret`.
#[unsafe(export_name = "_WMTRetireMetalView")]
pub unsafe extern "C-unwind" fn wmt_retire_metal_view(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 160 carries `UnixcallGenericObjUint64Noret`.
        let p = unsafe { params::<UnixcallGenericObjUint64Noret>(args) };
        if p.handle == 0 {
            return STATUS_SUCCESS;
        }
        let record = detach_metal_view_if_attached(p.handle);
        let view = u64_to_usize(p.handle);
        let want_park =
            (p.arg & u64::from(WMT_RETIRE_METAL_VIEW_FLAG_PARK) != 0) && has_cocoa_window();
        let park_record = record
            .as_ref()
            .filter(|att| want_park && att.hwnd() != 0 && att.view() == view);
        let Some(funcs) = MacdrvFuncs::load() else {
            return STATUS_SUCCESS;
        };
        if let Some(att) = park_record {
            prepare_parked_view(view, att.layer());
            let displaced = PARKED_METAL_VIEW
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .park(att.hwnd(), view, att.layer());
            if let Some(old_view) = displaced {
                release_raw_metal_view(funcs.release_metal_view, old_view);
            }
        } else {
            release_raw_metal_view(funcs.release_metal_view, view);
        }
        STATUS_SUCCESS
    })
}

/// Slot 161, `_WMTViewSyncAndPoll`: syncs `drawableSize` off-main and returns lock-free poll state.
///
/// # Safety
///
/// `args` is slot 161's `unixcall_wmt_view_sync_and_poll`.
#[unsafe(export_name = "_WMTViewSyncAndPoll")]
pub unsafe extern "C-unwind" fn wmt_view_sync_and_poll(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 161 carries `UnixcallWmtViewSyncAndPoll`.
        let p = unsafe { params::<UnixcallWmtViewSyncAndPoll>(args) };
        let att = if p.view != 0 {
            attachment::find(u64_to_usize(p.view))
        } else {
            None
        };
        let layer_handle = if p.layer != 0 {
            p.layer
        } else {
            att.as_ref().map_or(0, |a| a.layer() as u64)
        };
        let (dw, dh) = if has_cocoa_window() && layer_handle != 0 {
            // SAFETY: caller provides a live `CAMetalLayer` handle or attachment keeps one live.
            unsafe { obj::<CAMetalLayer>(layer_handle) }.map_or((0, 0), sync_drawable_size)
        } else {
            (0, 0)
        };
        let force = p.flags & WMT_VIEW_SYNC_FLAG_FORCE_REFRESH != 0;
        if let Some(att) = &att {
            if att.claim_refresh_if_due(force) {
                let refresh_att = Arc::clone(att);
                run_on_main_thread_async(move || refresh_attachment_on_main(&refresh_att));
            }
            // SAFETY: caller provides writable `WMTViewPollOut` or null.
            if let Some(out) = unsafe { mem_mut::<WMTViewPollOut>(p.out) } {
                att.write_poll_out(dw, dh, out);
            }
        } else {
            // SAFETY: caller provides writable `WMTViewPollOut` or null.
            if let Some(out) = unsafe { mem_mut::<WMTViewPollOut>(p.out) } {
                *out = WMTViewPollOut {
                    drawable_width: dw,
                    drawable_height: dh,
                    backing_scale: 1,
                    edr_current: 1.0,
                    edr_potential: 1.0,
                    panel_max_hz: 0,
                    flags: 0,
                    reserved: 0,
                };
            }
        }
        cursor_overlay::poll_from_present();
        STATUS_SUCCESS
    })
}

/// Slot 162, `_WMTPresentClientSurface`: presents the newest client surface on the view's window.
///
/// # Safety
///
/// `args` is slot 162's `unixcall_generic_obj_uint64_noret`.
#[unsafe(export_name = "_WMTPresentClientSurface")]
pub unsafe extern "C-unwind" fn wmt_present_client_surface(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 162 carries `UnixcallGenericObjUint64Noret`.
        let p = unsafe { params::<UnixcallGenericObjUint64Noret>(args) };
        if p.handle == 0 {
            return STATUS_SUCCESS;
        }
        let Some(att) = attachment::find(u64_to_usize(p.handle)) else {
            return STATUS_SUCCESS;
        };
        let Some(calls) = client_surface::SurfaceCalls::load() else {
            return STATUS_SUCCESS;
        };
        if let Some(surface) = attachment::retain_newest_surface_on(att.hwnd(), |s| calls.retain(s))
        {
            calls.present(surface);
            calls.release(surface);
        }
        STATUS_SUCCESS
    })
}

static SHARED_EVENT_SIGNAL_LOCK: Mutex<()> = Mutex::new(());

struct SendSharedEvent(AtomicUsize);

impl SendSharedEvent {
    fn from_retained(event: Option<Retained<ProtocolObject<dyn MTLSharedEvent>>>) -> Self {
        let addr = event.map_or(0, |e| Retained::into_raw(e) as usize);
        Self(AtomicUsize::new(addr))
    }

    fn signal(&self, value: u64) {
        let addr = self.0.load(Ordering::Acquire);
        if addr == 0 {
            return;
        }
        let _lock = SHARED_EVENT_SIGNAL_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `addr` came from `Retained::into_raw` and remains owned until `Drop`.
        let event = unsafe { &*(addr as *const ProtocolObject<dyn MTLSharedEvent>) };
        if event.signaledValue() < value {
            event.setSignaledValue(value);
        }
    }
}

impl Drop for SendSharedEvent {
    fn drop(&mut self) {
        let addr = self.0.swap(0, Ordering::AcqRel);
        if addr != 0 {
            autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                // SAFETY: `addr` came from `Retained::into_raw` and is released once on Drop.
                let _ =
                    unsafe { Retained::from_raw(addr as *mut ProtocolObject<dyn MTLSharedEvent>) };
            });
        }
    }
}

struct PresentedSignalState {
    event: SendSharedEvent,
    signal_value: u64,
    signaled: AtomicBool,
    att: Option<Arc<Attachment>>,
    paced: bool,
}

impl PresentedSignalState {
    fn fire(&self, displayed: bool) {
        if self.signaled.swap(true, Ordering::AcqRel) {
            return;
        }
        if self.paced
            && let Some(att) = &self.att
        {
            let _ = att.note_paced_present(displayed);
        }
        self.event.signal(self.signal_value);
    }
}

struct PresentedDropGuard(Arc<PresentedSignalState>);

impl Drop for PresentedDropGuard {
    fn drop(&mut self) {
        autoreleasepool(|_| {
            let _guard = AbortOnPanic;
            self.0.fire(false);
        });
    }
}

/// Slot 163, `_MTLDrawable_setPresentedEvent`: signals `shared_event` on present or Drop.
///
/// # Safety
///
/// `args` is slot 163's `unixcall_mtldrawable_setpresentedevent`.
#[unsafe(export_name = "_MTLDrawable_setPresentedEvent")]
pub unsafe extern "C-unwind" fn mtl_drawable_set_presented_event(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 163 carries `UnixcallMtldrawableSetpresentedevent`.
        let p = unsafe { params::<UnixcallMtldrawableSetpresentedevent>(args) };
        let paced = p.flags & WMT_PRESENTED_EVENT_FLAG_PACED != 0;
        let att = if p.view != 0 {
            attachment::find(u64_to_usize(p.view))
        } else {
            None
        };
        let event = if p.shared_event != 0 {
            // SAFETY: caller provides a live `MTLSharedEvent` handle or 0.
            unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.shared_event) }
                // SAFETY: retaining the live shared event for asynchronous Metal callback use.
                .and_then(|e| unsafe { Retained::retain(core::ptr::from_ref(e).cast_mut()) })
        } else {
            None
        };
        let state = Arc::new(PresentedSignalState {
            event: SendSharedEvent::from_retained(event),
            signal_value: p.signal_value,
            signaled: AtomicBool::new(false),
            att,
            paced,
        });
        // SAFETY: caller provides a live `CAMetalDrawable` handle or 0.
        let drawable = unsafe { obj::<ProtocolObject<dyn CAMetalDrawable>>(p.drawable) };
        if let Some(drawable) = drawable {
            let guard = PresentedDropGuard(state);
            let block = RcBlock::new(move |d_ptr: NonNull<ProtocolObject<dyn MTLDrawable>>| {
                autoreleasepool(|_| {
                    let _guard = AbortOnPanic;
                    // SAFETY: Metal invokes `addPresentedHandler:` with a live drawable.
                    let displayed = unsafe { d_ptr.as_ref() }.presentedTime() > 0.0;
                    guard.0.fire(displayed);
                });
            });
            // SAFETY: Metal copies the block onto the drawable; its `Drop` guard guarantees
            // signaling even if the drawable is discarded without calling the handler.
            unsafe { drawable.addPresentedHandler(RcBlock::as_ptr(&block)) };
        } else {
            state.fire(false);
        }
        STATUS_SUCCESS
    })
}
