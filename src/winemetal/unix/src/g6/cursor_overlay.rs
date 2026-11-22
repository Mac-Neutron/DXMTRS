//! Software cursor overlay: transparent borderless `NSWindow` and minimal Metal shader pass.
//!
//! Exports slot 164 (`WMTSetCursorOverlay`) and maintains one process-lifetime overlay window
//! whose sublayer tracks the pointer in a pre-commit `CFRunLoopObserver`, applying sRGB/BT.2446
//! tone mapping and optional gamma LUTs in a standalone Metal shader pass.

use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
};

use block2::RcBlock;
use objc2::{
    AnyThread, MainThreadMarker, MainThreadOnly,
    rc::{Retained, autoreleasepool},
    runtime::{AnyObject, ProtocolObject},
};
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSBackingStoreType, NSBitmapImageRep, NSColor,
    NSCursor, NSDeviceRGBColorSpace, NSEvent, NSImage, NSScreen, NSView, NSWindow,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_core_foundation::{
    CFData, CFRetained, CFRunLoop, CFRunLoopActivity, CFRunLoopObserver, CGPoint, CGRect, CGSize,
    kCFRunLoopCommonModes,
};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGImageComponentInfo, kCGColorSpaceExtendedLinearSRGB, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSDictionary, NSNotification, NSNotificationCenter, NSNull, NSString};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLClearColor, MTLCommandBuffer, MTLCommandBufferStatus,
    MTLCommandEncoder, MTLCommandQueue, MTLCompileOptions, MTLDevice, MTLLibrary, MTLLoadAction,
    MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResource,
    MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use objc2_quartz_core::{CAAction, CALayer, CAMetalLayer};
use rustc_hash::FxHashMap;
use winemetal_wire::{
    UnixcallGenericObjConstptrNoret, WMT_CURSOR_OVERLAY_FLAG_HARDWARE,
    WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN, WMT_CURSOR_OVERLAY_FLAG_VISIBLE, WMTCursorOverlayParams,
};
use xxhash_rust::xxh3::Xxh3;

use super::attachment::{self, Attachment};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_ref, mem_slice, params},
    float_cast::u64_to_usize,
};

const SPRITE_CACHE_ENTRIES: usize = 64;
const GAMMA_LANES: usize = 256 * 4;
const PARKED: CGPoint = CGPoint {
    x: -100_000.0,
    y: -100_000.0,
};

const SUBMITTED: u8 = 0;
const COMPLETED: u8 = 1;
const FAILED: u8 = 2;

const VIS_WANTED: u8 = 1 << 0;
const VIS_APP_ACTIVE: u8 = 1 << 1;
const VIS_POINTER_INSIDE: u8 = 1 << 2;
const VIS_OCCLUDED: u8 = 1 << 3;
const VIS_MINIATURIZED: u8 = 1 << 4;

const CURSOR_MSL: &str = r"#include <metal_stdlib>
using namespace metal;
struct VsOut { float4 position [[position]]; float2 uv; };
struct CursorUniforms {
    float l_hdr_nits; float p_hdr; float log2_p_hdr; float inv_p_minus_one;
    uint mode; uint has_gamma; uint pad0; uint pad1;
};
vertex VsOut wmt_cursor_vs(uint vid [[vertex_id]]) {
    float2 pos = float2(float(vid & 1u) * 4.0 - 1.0, float(vid & 2u) * 2.0 - 1.0);
    VsOut out;
    out.position = float4(pos, 0.0, 1.0);
    out.uv = float2((pos.x + 1.0) * 0.5, (1.0 - pos.y) * 0.5);
    return out;
}
constant float3x3 M_BT709_TO_LMS = float3x3(
    float3(0.29581979f, 0.15626526f, 0.03517609f),
    float3(0.62309210f, 0.72723389f, 0.15647891f),
    float3(0.08108478f, 0.11649609f, 0.80834595f));
constant float3x3 M_LMS_TO_BT709 = float3x3(
    float3( 6.17295074f, -1.32362933f, -0.01180843f),
    float3(-5.31983948f,  2.56024170f, -0.26411438f),
    float3( 0.14656067f, -0.23634644f,  1.27617645f));
constant float3x3 M_LMS_PQ_TO_ICTCP = float3x3(
    float3(0.5f,  1.61370003f,  4.37806224f),
    float3(0.5f, -3.32339620f, -4.24553966f),
    float3(0.0f,  1.70969617f, -0.13252264f));
constant float3x3 M_ICTCP_TO_LMS_PQ = float3x3(
    float3(1.0f,  1.0f,         1.0f),
    float3(0.00860903f, -0.00860903f,  0.56003270f),
    float3(0.11102963f, -0.11102963f, -0.32062717f));
inline float3 pq_oetf(float3 nits) {
    float3 y = pow(max(nits, float3(0.0f)) / 10000.0f, float3(2610.0f / 16384.0f));
    return pow((float3(3424.0f / 4096.0f) + (2413.0f / 128.0f) * y) /
               (float3(1.0f) + (2392.0f / 128.0f) * y), float3(2523.0f / 32.0f));
}
inline float3 pq_eotf(float3 pq) {
    float3 e = pow(max(pq, float3(0.0f)), float3(32.0f / 2523.0f));
    float3 num = max(e - float3(3424.0f / 4096.0f), float3(0.0f));
    float3 den = max(float3(2413.0f / 128.0f) - (2392.0f / 128.0f) * e, float3(1e-20f));
    return 10000.0f * pow(num / den, float3(16384.0f / 2610.0f));
}
inline float3 srgb_eotf(float3 c) {
    return select(pow((c + 0.055f) / 1.055f, float3(2.4f)), c / 12.92f, c <= float3(0.04045f));
}
inline float3 apply_gamma(float3 c, constant ushort4* lut) {
    float3 x = clamp(c, 0.0f, 1.0f) * 255.0f;
    float3 lo = floor(x);
    uint3 i0 = uint3(lo);
    uint3 i1 = min(i0 + 1u, uint3(255u));
    float3 a = float3(lut[i0.x].r, lut[i0.y].g, lut[i0.z].b);
    float3 b = float3(lut[i1.x].r, lut[i1.y].g, lut[i1.z].b);
    return mix(a, b, x - lo) * (1.0f / 65535.0f);
}
inline float3 bt2446_ictcp(float3 lin, constant CursorUniforms& u) {
    float y_sdr = max(dot(lin, float3(0.2126f, 0.7152f, 0.0722f)), 1e-20f);
    float yp_c = log((pow(y_sdr, 1.0f / 2.4f) * 4.43952847f) + 1.0f) / 1.6937747f;
    float yp_0 = yp_c / 1.0770f;
    float yp_1 = (-2.7811f + sqrt(4.83307641f - 4.604f * yp_c)) / -2.302f;
    float yp_2 = (yp_c - 0.5f) / 0.5f;
    float yp_p = yp_0 <= 0.7399f ? yp_0 : (yp_2 >= 0.9909f ? yp_2 : yp_1);
    float y_hdr = pow((exp2(yp_p * u.log2_p_hdr) - 1.0f) * u.inv_p_minus_one, 2.4f) * u.l_hdr_nits;
    float3 ictcp = M_LMS_PQ_TO_ICTCP * pq_oetf(M_BT709_TO_LMS * (lin * 100.0f));
    float i_hdr = pq_oetf(float3(y_hdr)).x;
    float r = i_hdr / max(ictcp.x, 1e-6f);
    return max(M_LMS_TO_BT709 * pq_eotf(M_ICTCP_TO_LMS_PQ * float3(i_hdr, ictcp.y * r, ictcp.z * r)), float3(0.0f)) / 100.0f;
}
fragment float4 wmt_cursor_ps(
    VsOut in [[stage_in]], texture2d<float> src [[texture(0)]],
    constant CursorUniforms& u [[buffer(0)]], constant ushort4* lut [[buffer(1)]]
) {
    constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
    float4 sample = src.sample(s, in.uv);
    float3 rgb = u.has_gamma ? apply_gamma(sample.rgb, lut) : sample.rgb;
    if (u.mode == 1u) rgb = srgb_eotf(rgb);
    else if (u.mode == 2u) rgb = bt2446_ictcp(srgb_eotf(rgb), u);
    return float4(rgb * sample.a, sample.a);
}
";

#[repr(C)]
struct CursorUniforms {
    l_hdr_nits: f32,
    p_hdr: f32,
    log2_p_hdr: f32,
    inv_p_minus_one: f32,
    mode: u32,
    has_gamma: u32,
    pad0: u32,
    pad1: u32,
}

fn hdr_uniforms(peak: f32, mode: u32, has_gamma: bool) -> CursorUniforms {
    let l_hdr_nits = 100.0 * peak.max(1.0);
    let p_hdr = 32.0_f32.mul_add((l_hdr_nits / 10_000.0_f32).powf(1.0 / 2.4), 1.0);
    let log2_p_hdr = p_hdr.log2();
    let inv_p_minus_one = 1.0 / (p_hdr - 1.0);
    CursorUniforms {
        l_hdr_nits,
        p_hdr,
        log2_p_hdr,
        inv_p_minus_one,
        mode,
        has_gamma: u32::from(has_gamma),
        pad0: 0,
        pad1: 0,
    }
}

struct BoundedMap<V> {
    cap: usize,
    order: VecDeque<u64>,
    entries: FxHashMap<u64, V>,
}

impl<V> BoundedMap<V> {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            order: VecDeque::with_capacity(cap),
            entries: FxHashMap::default(),
        }
    }

    fn contains(&self, key: u64) -> bool {
        self.entries.contains_key(&key)
    }

    fn get(&self, key: u64) -> Option<&V> {
        self.entries.get(&key)
    }

    fn insert(&mut self, key: u64, value: V) {
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.entries.entry(key) {
            e.insert(value);
            return;
        }
        while self.entries.len() >= self.cap {
            if let Some(evict) = self.order.pop_front() {
                self.entries.remove(&evict);
            } else {
                break;
            }
        }
        self.order.push_back(key);
        self.entries.insert(key, value);
    }

    fn clear(&mut self) {
        self.order.clear();
        self.entries.clear();
    }
}

struct Sprite {
    width: u32,
    height: u32,
    x_hotspot: u32,
    y_hotspot: u32,
    pixels: Box<[u8]>,
}

/// Hash cursor sprite dimensions, hotspot, and pixel bytes cleanly, mapping `0` to `1`.
#[must_use]
pub fn sprite_hash(width: u32, height: u32, x_hotspot: u32, y_hotspot: u32, pixels: &[u8]) -> u64 {
    let mut hasher = Xxh3::new();
    hasher.update(&width.to_le_bytes());
    hasher.update(&height.to_le_bytes());
    hasher.update(&x_hotspot.to_le_bytes());
    hasher.update(&y_hotspot.to_le_bytes());
    hasher.update(pixels);
    let h = hasher.digest();
    if h == 0 { 1 } else { h }
}

struct Shared {
    sprites: BoundedMap<Arc<Sprite>>,
    last_hash_by_view: FxHashMap<usize, u64>,
    owner: Option<Arc<Attachment>>,
    hash: u64,
    flags: u32,
    gamma_revision: u64,
    gamma_lut: Option<[u16; GAMMA_LANES]>,
    pending: bool,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            sprites: BoundedMap::new(SPRITE_CACHE_ENTRIES),
            last_hash_by_view: FxHashMap::default(),
            owner: None,
            hash: 0,
            flags: 0,
            gamma_revision: 0,
            gamma_lut: None,
            pending: false,
        }
    }
}

struct WantedSnapshot {
    owner: Option<Arc<Attachment>>,
    sprite: Option<Arc<Sprite>>,
    hash: u64,
    flags: u32,
    gamma_revision: u64,
    gamma_lut: Option<Arc<[u16; GAMMA_LANES]>>,
}

impl Shared {
    fn update(
        &mut self,
        view: usize,
        p: &WMTCursorOverlayParams,
        pixels: Option<&[u8]>,
        gamma_lut: Option<&[u16; GAMMA_LANES]>,
    ) -> bool {
        let Some(owner) = attachment::find(view) else {
            return false;
        };
        let hardware = p.flags & WMT_CURSOR_OVERLAY_FLAG_HARDWARE != 0;
        let same_owner = self
            .owner
            .as_ref()
            .is_some_and(|prev| Arc::ptr_eq(prev, &owner));
        let next_hash = if hardware {
            0
        } else if let Some(bytes) = pixels {
            let h = sprite_hash(p.width, p.height, p.x_hotspot, p.y_hotspot, bytes);
            if !self.sprites.contains(h) {
                self.sprites.insert(
                    h,
                    Arc::new(Sprite {
                        width: p.width,
                        height: p.height,
                        x_hotspot: p.x_hotspot,
                        y_hotspot: p.y_hotspot,
                        pixels: bytes.into(),
                    }),
                );
            }
            self.last_hash_by_view.insert(view, h);
            h
        } else if let Some(h) = self
            .last_hash_by_view
            .get(&view)
            .copied()
            .filter(|h| self.sprites.contains(*h))
        {
            h
        } else if same_owner && self.sprites.contains(self.hash) {
            self.hash
        } else {
            0
        };
        let next_gamma_rev = if gamma_lut.is_some() {
            p.gamma_revision
        } else {
            0
        };
        if same_owner
            && self.hash == next_hash
            && self.flags == p.flags
            && self.gamma_revision == next_gamma_rev
        {
            return self.pending;
        }
        let native_was_hidden = self.flags & WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN != 0;
        self.owner = Some(owner);
        self.hash = next_hash;
        self.flags = p.flags;
        self.gamma_revision = next_gamma_rev;
        self.gamma_lut = gamma_lut.copied();
        self.pending = !hardware
            || native_was_hidden
            || (p.flags & WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN != 0)
            || next_hash == 0;
        self.pending
    }

    fn detach(&mut self, retired: &Arc<Attachment>) -> bool {
        self.last_hash_by_view.remove(&retired.view());
        let Some(owner) = self.owner.as_ref() else {
            return false;
        };
        if !Arc::ptr_eq(owner, retired) {
            return false;
        }
        self.owner = None;
        self.hash = 0;
        self.flags = 0;
        self.gamma_revision = 0;
        self.gamma_lut = None;
        self.pending = true;
        true
    }

    fn snapshot(&self) -> WantedSnapshot {
        let sprite = if self.hash != 0 {
            self.sprites.get(self.hash).map(Arc::clone)
        } else {
            None
        };
        WantedSnapshot {
            owner: self.owner.as_ref().map(Arc::clone),
            sprite,
            hash: self.hash,
            flags: self.flags,
            gamma_revision: self.gamma_revision,
            gamma_lut: self.gamma_lut.map(Arc::new),
        }
    }
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(|| Mutex::new(Shared::default()));
static APPLY_PENDING: AtomicBool = AtomicBool::new(false);
static RECONCILE_REQUESTED: AtomicBool = AtomicBool::new(false);

fn lock_shared() -> MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Slot 164, `_WMTSetCursorOverlay`: updates the software cursor bitmap and visibility.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_constptr_noret`, its `arg` pointing to a live
/// `WMTCursorOverlayParams` or null.
#[unsafe(export_name = "_WMTSetCursorOverlay")]
pub unsafe extern "C-unwind" fn wmt_set_cursor_overlay(args: *mut c_void) -> i32 {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        // SAFETY: slot 164 carries `UnixcallGenericObjConstptrNoret`.
        let p = unsafe { params::<UnixcallGenericObjConstptrNoret>(args) };
        // SAFETY: caller ensures `p.arg` points to a readable `WMTCursorOverlayParams` for the call.
        let Some(cfg) = (unsafe { mem_ref::<WMTCursorOverlayParams>(p.arg) }) else {
            return STATUS_SUCCESS;
        };
        let view_handle = if cfg.view != 0 { cfg.view } else { p.handle };
        let view = u64_to_usize(view_handle);
        let hardware = cfg.flags & WMT_CURSOR_OVERLAY_FLAG_HARDWARE != 0;
        let pixels = if !hardware && cfg.pixels.ptr != 0 && cfg.pixels_len != 0 {
            let valid_dims = cfg.width > 0
                && cfg.height > 0
                && cfg.width <= 4096
                && cfg.height <= 4096
                && u64::from(cfg.width) * u64::from(cfg.height) * 4 == u64::from(cfg.pixels_len);
            if !valid_dims {
                return STATUS_SUCCESS;
            }
            // SAFETY: caller provides `pixels_len` readable BGRA8 bytes for this synchronous call.
            unsafe { mem_slice::<u8>(cfg.pixels, u64::from(cfg.pixels_len)) }
        } else {
            None
        };
        let gamma_lut_owned: Option<[u16; GAMMA_LANES]> = if cfg.gamma_lut.ptr != 0
            && cfg.gamma_revision != 0
        {
            // SAFETY: caller provides `GAMMA_LANES * 2` bytes for 256 RGBA `u16` lanes.
            unsafe { mem_slice::<u8>(cfg.gamma_lut, (GAMMA_LANES * 2) as u64) }.and_then(|bytes| {
                let (chunks, []) = bytes.as_chunks::<2>() else {
                    return None;
                };
                (chunks.len() == GAMMA_LANES)
                    .then(|| core::array::from_fn(|i| u16::from_ne_bytes(chunks[i])))
            })
        } else {
            None
        };
        let wake = lock_shared().update(view, cfg, pixels, gamma_lut_owned.as_ref());
        if wake {
            queue_apply();
        }
        STATUS_SUCCESS
    })
}

/// Notify the overlay that `retired` is leaving.
pub fn detach(retired: &Arc<Attachment>) {
    if lock_shared().detach(retired) {
        queue_apply();
    }
}

/// Request a coalesced pre-commit cursor reconciliation during present.
pub fn poll_from_present() {
    let check = {
        let shared = lock_shared();
        shared.pending
            || (shared.owner.is_some()
                && shared.flags
                    & (WMT_CURSOR_OVERLAY_FLAG_VISIBLE | WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN)
                    != 0)
    };
    if check {
        queue_apply();
    }
}

/// Trigger one main-thread cursor reconciliation when watchers or attachments change.
pub fn reconcile_on_main() {
    if let Some(mtm) = MainThreadMarker::new() {
        install_pointer_watch(mtm);
        RECONCILE_REQUESTED.store(true, Ordering::Release);
    }
}

fn queue_apply() {
    RECONCILE_REQUESTED.store(true, Ordering::Release);
    if APPLY_PENDING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        super::window::run_on_main_thread_async(apply_on_main);
    }
}

fn apply_on_main() {
    APPLY_PENDING.store(false, Ordering::Release);
    if let Some(mtm) = MainThreadMarker::new() {
        install_pointer_watch(mtm);
    }
}

thread_local! {
    static POINTER_WATCH_INSTALLED: Cell<u8> = const { Cell::new(0) };
    static POINTER_WATCH: RefCell<PointerWatch> = RefCell::new(PointerWatch::default());
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

const WATCH_RUN_LOOP: u8 = 1 << 0;
const WATCH_ACTIVATION: u8 = 1 << 1;

#[derive(Default)]
struct PointerWatch {
    native_cursor: Option<Retained<NSCursor>>,
    captured: bool,
    last_location: CGPoint,
}

/// Install the process-lifetime run-loop and activation observers for cursor tracking.
pub fn install_pointer_watch(mtm: MainThreadMarker) {
    let mut installed = POINTER_WATCH_INSTALLED.get();
    if installed & WATCH_RUN_LOOP == 0 {
        let activities = CFRunLoopActivity::BeforeWaiting | CFRunLoopActivity::Exit;
        // SAFETY: `before_commit` runs only on the main run loop with no external context pointer.
        let observer = unsafe {
            CFRunLoopObserver::new(
                None,
                activities.0,
                true,
                0,
                Some(before_commit),
                core::ptr::null_mut(),
            )
        };
        // SAFETY: process-lifetime CoreFoundation constant.
        let common_modes = unsafe { kCFRunLoopCommonModes };
        if let (Some(observer), Some(main_loop)) = (observer, CFRunLoop::main()) {
            main_loop.add_observer(Some(&observer), common_modes);
            core::mem::forget(observer);
            installed |= WATCH_RUN_LOOP;
        }
    }
    if installed & WATCH_ACTIVATION == 0 {
        install_activation_watch(mtm);
        installed |= WATCH_ACTIVATION;
    }
    POINTER_WATCH_INSTALLED.set(installed);
}

fn install_activation_watch(_mtm: MainThreadMarker) {
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: process-lifetime AppKit notification constant.
    let became = unsafe { NSApplicationDidBecomeActiveNotification };
    // SAFETY: process-lifetime AppKit notification constant.
    let resigned = unsafe { NSApplicationDidResignActiveNotification };
    for name in [became, resigned] {
        let block = RcBlock::new(|_: NonNull<NSNotification>| {
            autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                queue_apply();
            });
        });
        // SAFETY: notification center copies the block; token is intentionally leaked.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
        };
        core::mem::forget(token);
    }
}

extern "C-unwind" fn before_commit(
    _observer: *mut CFRunLoopObserver,
    _activity: CFRunLoopActivity,
    _info: *mut c_void,
) {
    autoreleasepool(|_| {
        let _guard = AbortOnPanic;
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if reconcile_due() {
            apply_on_main_inner(mtm);
        }
    });
}

fn reconcile_due() -> bool {
    let requested = RECONCILE_REQUESTED.swap(false, Ordering::AcqRel);
    let location = NSEvent::mouseLocation();
    let moved = POINTER_WATCH.with_borrow_mut(|watch| {
        let moved = watch.last_location != location;
        watch.last_location = location;
        moved
    });
    moved || requested
}

struct PointerHit {
    window: Retained<NSWindow>,
    mouse: CGPoint,
    over_game: bool,
    captured: bool,
    miniaturized: bool,
}

fn pointer_hit(mtm: MainThreadMarker, wanted: &WantedSnapshot, active: bool) -> Option<PointerHit> {
    if !active
        || (wanted.flags
            & (WMT_CURSOR_OVERLAY_FLAG_VISIBLE | WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN)
            == 0)
    {
        return None;
    }
    let view = attachment::retain_view(wanted.owner.as_ref()?, mtm)?;
    let window = view.window()?;
    let mouse = NSEvent::mouseLocation();
    let client = window.convertRectToScreen(view.convertRect_toView(view.bounds(), None));
    let inside = mouse.x >= client.origin.x
        && mouse.y >= client.origin.y
        && mouse.x < client.origin.x + client.size.width
        && mouse.y < client.origin.y + client.size.height;
    let hit = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(mouse, 0, mtm);
    let hit_is_game = hit == window.windowNumber();
    let hit_is_ours = hit_is_game
        || NSApplication::sharedApplication(mtm)
            .windowWithWindowNumber(hit)
            .is_some();
    Some(PointerHit {
        miniaturized: window.isMiniaturized(),
        window,
        mouse,
        over_game: inside && hit_is_game,
        captured: inside && !hit_is_game && !hit_is_ours,
    })
}

fn apply_on_main_inner(mtm: MainThreadMarker) {
    let wanted = {
        let shared = lock_shared();
        if shared.owner.is_none() && !shared.pending {
            return;
        }
        shared.snapshot()
    };
    install_pointer_watch(mtm);
    let active = NSApplication::sharedApplication(mtm).isActive();
    let hit = pointer_hit(mtm, &wanted, active);
    let captured = hit.as_ref().is_some_and(|h| h.captured);
    POINTER_WATCH.with_borrow_mut(|watch| {
        if watch.captured != captured {
            watch.captured = captured;
            if !captured {
                attachment::request_cursor_kick_all();
            }
        }
    });
    if wanted.flags & WMT_CURSOR_OVERLAY_FLAG_HARDWARE == 0
        && wanted.flags & WMT_CURSOR_OVERLAY_FLAG_NATIVE_HIDDEN != 0
        && let (Some(att), Some(hit)) = (wanted.owner.as_ref(), hit.as_ref())
        && hit.over_game
        && !hit.miniaturized
        && !att.window_occluded()
    {
        select_native_blank();
    }
    let completed = OVERLAY.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            return false;
        };
        if slot.is_none() {
            if wanted.sprite.is_none() {
                return true;
            }
            *slot = Overlay::create(mtm, &wanted);
        }
        slot.as_mut()
            .is_some_and(|overlay| overlay.apply(mtm, &wanted, active, hit.as_ref()))
    });
    let mut shared = lock_shared();
    if shared.owner.as_ref().map(Arc::as_ptr) == wanted.owner.as_ref().map(Arc::as_ptr)
        && shared.hash == wanted.hash
        && shared.flags == wanted.flags
        && shared.gamma_revision == wanted.gamma_revision
        && completed
    {
        shared.pending = false;
    }
}

#[derive(PartialEq)]
struct SpriteKey {
    hash: u64,
    hdr: bool,
    peak: f32,
    width_pt: f64,
    height_pt: f64,
    hotspot_x_pt: f64,
    hotspot_y_pt: f64,
    scale: f64,
    gamma_revision: u64,
}

impl SpriteKey {
    fn from_sprite(
        sprite: &Sprite,
        hash: u64,
        hdr: bool,
        peak: f32,
        backing_scale: u32,
        gamma_revision: u64,
    ) -> Self {
        let scale = f64::from(backing_scale.max(1));
        Self {
            hash,
            hdr,
            peak,
            width_pt: f64::from(sprite.width) / scale,
            height_pt: f64::from(sprite.height) / scale,
            hotspot_x_pt: f64::from(sprite.x_hotspot) / scale,
            hotspot_y_pt: f64::from(sprite.y_hotspot) / scale,
            scale,
            gamma_revision,
        }
    }
}

struct CursorDraw {
    command: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

struct Overlay {
    window: Retained<NSWindow>,
    layer: Retained<CAMetalLayer>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipelines: FxHashMap<usize, Retained<ProtocolObject<dyn MTLRenderPipelineState>>>,
    textures: BoundedMap<Retained<ProtocolObject<dyn MTLTexture>>>,
    images: BoundedMap<(SpriteKey, CFRetained<CGImage>)>,
    transparent: CFRetained<CGImage>,
    pending_key: Option<SpriteKey>,
    pending_result: Option<Arc<AtomicU8>>,
    pending_draw: Option<CursorDraw>,
    published_key: Option<SpriteKey>,
    owner: Option<Arc<Attachment>>,
    hdr_mode: Option<bool>,
}

impl Overlay {
    fn create(mtm: MainThreadMarker, wanted: &WantedSnapshot) -> Option<Self> {
        let transparent = transparent_image()?;
        let game_layer = wanted
            .owner
            .as_ref()
            .and_then(|att| attachment::retain_layer(att, mtm))?;
        let device = game_layer.device()?;
        let queue = device.newCommandQueue()?;
        queue.setLabel(Some(&NSString::from_str("wmt-cursor-queue")));

        let layer = CAMetalLayer::new();
        layer.setDevice(Some(&device));
        layer.setOpaque(false);
        layer.setActions(Some(&no_actions()));
        layer.setName(Some(&NSString::from_str("wmt-cursor-overlay")));
        layer.setAnchorPoint(CGPoint { x: 0.0, y: 0.0 });
        layer.setPosition(PARKED);
        layer.setBounds(CGRect {
            origin: CGPoint::default(),
            size: CGSize {
                width: 1.0,
                height: 1.0,
            },
        });
        disable_hud_on_layer(&layer);

        let game_win = game_window(mtm, wanted);
        let frame = game_win
            .as_deref()
            .map(NSWindow::frame)
            .or_else(|| NSScreen::mainScreen(mtm).map(|s| s.frame()))
            .unwrap_or_else(|| CGRect {
                origin: CGPoint::default(),
                size: CGSize {
                    width: 1.0,
                    height: 1.0,
                },
            });
        // SAFETY: borderless buffered overlay window initialized with `defer = false`.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: window is kept in `OVERLAY` for process lifetime and never closed.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setIgnoresMouseEvents(true);
        window.setHasShadow(false);
        window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        let view = NSView::initWithFrame(
            NSView::alloc(mtm),
            CGRect {
                origin: CGPoint::default(),
                size: frame.size,
            },
        );
        let host = CALayer::new();
        host.addSublayer(&layer);
        view.setLayer(Some(&host));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));

        Some(Self {
            window,
            layer,
            queue,
            pipelines: FxHashMap::default(),
            textures: BoundedMap::new(SPRITE_CACHE_ENTRIES),
            images: BoundedMap::new(SPRITE_CACHE_ENTRIES),
            transparent,
            pending_key: None,
            pending_result: None,
            pending_draw: None,
            published_key: None,
            owner: None,
            hdr_mode: None,
        })
    }

    fn apply(
        &mut self,
        mtm: MainThreadMarker,
        wanted: &WantedSnapshot,
        active: bool,
        hit: Option<&PointerHit>,
    ) -> bool {
        self.follow_window(mtm, wanted);
        let same_owner = self
            .owner
            .as_ref()
            .zip(wanted.owner.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        if !same_owner {
            self.invalidate_pending();
            self.owner = wanted.owner.as_ref().map(Arc::clone);
        }
        if let Some(att) = wanted.owner.as_ref() {
            let Some(game_layer) = attachment::retain_layer(att, mtm) else {
                self.publish_transparent();
                return false;
            };
            if !self.reconfigure_layer(&game_layer, att) {
                self.publish_transparent();
                return false;
            }
        }
        self.sync_position(wanted, active, hit)
    }

    fn invalidate_pending(&mut self) {
        self.pending_key = None;
        self.pending_result = None;
        self.pending_draw = None;
    }

    fn publish_transparent(&mut self) {
        self.invalidate_pending();
        set_layer_image(&self.layer, &self.transparent);
        self.published_key = None;
    }

    fn follow_window(&self, mtm: MainThreadMarker, wanted: &WantedSnapshot) {
        let window = game_window(mtm, wanted);
        let parent = self.window.parentWindow();
        if parent.as_deref() != window.as_deref() {
            if let Some(parent) = parent {
                parent.removeChildWindow(&self.window);
            }
            if let Some(win) = window.as_ref() {
                self.window.setLevel(win.level() + 1);
                // SAFETY: parent and overlay are live main-thread windows with no child cycle.
                unsafe { win.addChildWindow_ordered(&self.window, NSWindowOrderingMode::Above) };
            }
        }
        if let Some(win) = window.as_deref() {
            let frame = win.frame();
            if self.window.frame() != frame {
                self.window.setFrame_display(frame, false);
            }
        }
        if window.is_some_and(|w| w.isVisible()) {
            if !self.window.isVisible() {
                self.window.orderFront(None);
            }
        } else if self.window.isVisible() {
            self.window.orderOut(None);
        }
    }

    fn reconfigure_layer(&mut self, game: &CAMetalLayer, att: &Attachment) -> bool {
        let Some(device) = game.device() else {
            return false;
        };
        if self.queue.device().registryID() != device.registryID() {
            let Some(queue) = device.newCommandQueue() else {
                return false;
            };
            queue.setLabel(Some(&NSString::from_str("wmt-cursor-queue")));
            self.queue = queue;
            self.layer.setDevice(Some(&device));
            self.pipelines.clear();
            self.textures.clear();
            self.images.clear();
            self.invalidate_pending();
        }
        let hdr = att.hdr_active();
        let next_fmt = game.pixelFormat();
        let next_cs = game.colorspace();
        let next_edr = game.wantsExtendedDynamicRangeContent();
        let cur_cs = self.layer.colorspace();
        let cs_same = match (cur_cs.as_deref(), next_cs.as_deref()) {
            (None, None) => true,
            (Some(a), Some(b)) => core::ptr::eq(a, b),
            _ => false,
        };
        if self.hdr_mode != Some(hdr)
            || self.layer.pixelFormat() != next_fmt
            || !cs_same
            || self.layer.wantsExtendedDynamicRangeContent() != next_edr
        {
            self.layer.setPixelFormat(next_fmt);
            self.layer.setColorspace(next_cs.as_deref());
            self.layer.setWantsExtendedDynamicRangeContent(next_edr);
            self.hdr_mode = Some(hdr);
            self.images.clear();
            self.invalidate_pending();
        }
        true
    }

    fn sync_position(
        &mut self,
        wanted: &WantedSnapshot,
        active: bool,
        hit: Option<&PointerHit>,
    ) -> bool {
        let Some(att) = wanted.owner.as_ref() else {
            self.publish_transparent();
            return true;
        };
        let mut vis = 0u8;
        if wanted.flags & WMT_CURSOR_OVERLAY_FLAG_VISIBLE != 0 && wanted.sprite.is_some() {
            vis |= VIS_WANTED;
        }
        if active {
            vis |= VIS_APP_ACTIVE;
        }
        if vis & (VIS_WANTED | VIS_APP_ACTIVE) != (VIS_WANTED | VIS_APP_ACTIVE) {
            self.publish_transparent();
            return true;
        }
        let Some(hit) = hit else {
            self.publish_transparent();
            return true;
        };
        let level = hit.window.level() + 1;
        if self.window.level() != level {
            self.window.setLevel(level);
        }
        if hit.over_game {
            vis |= VIS_POINTER_INSIDE;
        }
        if att.window_occluded() {
            vis |= VIS_OCCLUDED;
        }
        if hit.miniaturized {
            vis |= VIS_MINIATURIZED;
        }
        let shown = (vis & (VIS_WANTED | VIS_APP_ACTIVE | VIS_POINTER_INSIDE))
            == (VIS_WANTED | VIS_APP_ACTIVE | VIS_POINTER_INSIDE)
            && (vis & (VIS_OCCLUDED | VIS_MINIATURIZED)) == 0;
        if shown {
            select_native_blank();
        }
        if !shown {
            self.publish_transparent();
            return true;
        }
        let Some(sprite) = wanted.sprite.as_deref() else {
            self.publish_transparent();
            return true;
        };
        let hdr = self.hdr_mode.unwrap_or(false);
        let raw_peak = att.headroom();
        let peak = match self.pending_key.as_ref() {
            Some(prev)
                if prev.hash == wanted.hash
                    && prev.hdr == hdr
                    && !peak_changed(prev.peak, raw_peak) =>
            {
                prev.peak
            }
            _ => raw_peak,
        };
        let key = SpriteKey::from_sprite(
            sprite,
            wanted.hash,
            hdr,
            peak,
            att.backing_scale(),
            wanted.gamma_revision,
        );
        let ready = self.ensure_sprite_image(&key, sprite, wanted.gamma_lut.as_deref());
        if hit.over_game {
            let local = self.window.convertPointFromScreen(hit.mouse);
            let geom = if ready {
                Some(&key)
            } else {
                self.published_key.as_ref()
            };
            if let Some(g) = geom {
                self.layer.setPosition(CGPoint {
                    x: local.x - g.hotspot_x_pt,
                    y: local.y - (g.height_pt - g.hotspot_y_pt),
                });
            }
        }
        ready
    }

    fn ensure_sprite_image(
        &mut self,
        key: &SpriteKey,
        sprite: &Sprite,
        gamma_lut: Option<&[u16; GAMMA_LANES]>,
    ) -> bool {
        if self.published_key.as_ref() == Some(key) {
            return true;
        }
        if let Some((cached_key, img)) = self.images.get(key.hash)
            && cached_key == key
        {
            apply_sprite_image(&self.layer, img, key);
            self.published_key = Some(clone_key(key));
            self.invalidate_pending();
            return true;
        }
        if self.pending_key.as_ref() == Some(key)
            && let Some(res) = self.pending_result.as_ref()
        {
            match res.load(Ordering::Acquire) {
                SUBMITTED => return false,
                COMPLETED => {
                    if let Some(draw) = self.pending_draw.take() {
                        let cs = self.layer.colorspace();
                        if let Some(img) = readback_texture(&draw.texture, cs.as_deref()) {
                            apply_sprite_image(&self.layer, &img, key);
                            self.images.insert(key.hash, (clone_key(key), img));
                            self.published_key = Some(clone_key(key));
                            self.invalidate_pending();
                            return true;
                        }
                    }
                    self.invalidate_pending();
                    return false;
                }
                _ => self.invalidate_pending(),
            }
        }
        let Some(draw) = self.render_sprite(key, sprite, gamma_lut) else {
            return false;
        };
        let result = Arc::new(AtomicU8::new(SUBMITTED));
        let done = Arc::clone(&result);
        let handler = RcBlock::new(move |ptr: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
            autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                // SAFETY: Metal provides a live completed command buffer for the callback.
                let buf = unsafe { ptr.as_ref() };
                let ok = buf.status() == MTLCommandBufferStatus::Completed;
                done.store(if ok { COMPLETED } else { FAILED }, Ordering::Release);
                queue_apply();
            });
        });
        // SAFETY: Metal copies the block before commit.
        unsafe { draw.command.addCompletedHandler(RcBlock::as_ptr(&handler)) };
        draw.command.commit();
        self.pending_key = Some(clone_key(key));
        self.pending_result = Some(result);
        self.pending_draw = Some(draw);
        false
    }

    fn render_sprite(
        &mut self,
        key: &SpriteKey,
        sprite: &Sprite,
        gamma_lut: Option<&[u16; GAMMA_LANES]>,
    ) -> Option<CursorDraw> {
        let device = self.queue.device();
        let storage = if device.hasUnifiedMemory() {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        };
        if !self.textures.contains(key.hash) {
            let tex = upload_sprite_texture(&device, sprite, storage)?;
            self.textures.insert(key.hash, tex);
        }
        let src = self.textures.get(key.hash)?;
        let fmt = self.layer.pixelFormat();
        let pipeline = ensure_pipeline(&device, fmt, &mut self.pipelines)?;
        let desc = MTLTextureDescriptor::new();
        desc.setTextureType(MTLTextureType::Type2D);
        desc.setPixelFormat(fmt);
        // SAFETY: positive validated cursor sprite width.
        unsafe { desc.setWidth(sprite.width as usize) };
        // SAFETY: positive validated cursor sprite height.
        unsafe { desc.setHeight(sprite.height as usize) };
        desc.setUsage(MTLTextureUsage::RenderTarget);
        desc.setStorageMode(storage);
        let output = device.newTextureWithDescriptor(&desc)?;
        output.setLabel(Some(&NSString::from_str("wmt-cursor-image")));
        let command = self.queue.commandBuffer()?;
        command.setLabel(Some(&NSString::from_str("wmt-cursor")));

        let pass_desc = MTLRenderPassDescriptor::new();
        // SAFETY: `colorAttachments()` index 0 is always present on a new descriptor.
        let color0 = unsafe { pass_desc.colorAttachments().objectAtIndexedSubscript(0) };
        color0.setTexture(Some(&output));
        color0.setLoadAction(MTLLoadAction::Clear);
        color0.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        });
        color0.setStoreAction(MTLStoreAction::Store);
        let enc = command.renderCommandEncoderWithDescriptor(&pass_desc)?;
        enc.setRenderPipelineState(&pipeline);
        // SAFETY: `src` is retained in `self.textures` for the command encoding.
        unsafe { enc.setFragmentTexture_atIndex(Some(src), 0) };
        let mode = if !key.hdr {
            0
        } else if key.peak <= 1.0 {
            1
        } else {
            2
        };
        let uniforms = hdr_uniforms(key.peak, mode, gamma_lut.is_some());
        let u_ptr = NonNull::from(&uniforms).cast::<c_void>();
        // SAFETY: `u_ptr` points to stack-allocated `CursorUniforms` copied by Metal before return.
        unsafe { enc.setFragmentBytes_length_atIndex(u_ptr, size_of::<CursorUniforms>(), 0) };
        let dummy = [0u16; 4];
        let (lut_ptr, lut_len) = gamma_lut.map_or_else(
            || (NonNull::from(&dummy).cast::<c_void>(), size_of_val(&dummy)),
            |lut| (NonNull::from(lut).cast::<c_void>(), size_of_val(lut)),
        );
        // SAFETY: `lut_ptr` points to live gamma data copied by Metal before return.
        unsafe { enc.setFragmentBytes_length_atIndex(lut_ptr, lut_len, 1) };
        // SAFETY: fullscreen triangle vertex shader synthesizes 3 vertices with no vertex buffers.
        unsafe { enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };
        enc.endEncoding();

        if output.storageMode() == MTLStorageMode::Managed {
            let blit = command.blitCommandEncoder()?;
            blit.synchronizeResource(ProtocolObject::from_ref(&*output));
            blit.endEncoding();
        }
        Some(CursorDraw {
            command,
            texture: output,
        })
    }
}

const fn clone_key(k: &SpriteKey) -> SpriteKey {
    SpriteKey {
        hash: k.hash,
        hdr: k.hdr,
        peak: k.peak,
        width_pt: k.width_pt,
        height_pt: k.height_pt,
        hotspot_x_pt: k.hotspot_x_pt,
        hotspot_y_pt: k.hotspot_y_pt,
        scale: k.scale,
        gamma_revision: k.gamma_revision,
    }
}

fn peak_changed(applied: f32, current: f32) -> bool {
    let relative = (current - applied).abs() / applied.max(f32::EPSILON);
    relative > 0.05 || (applied <= 1.0) != (current <= 1.0)
}

fn ensure_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    format: MTLPixelFormat,
    cache: &mut FxHashMap<usize, Retained<ProtocolObject<dyn MTLRenderPipelineState>>>,
) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    if let Some(p) = cache.get(&format.0) {
        return Some(Retained::clone(p));
    }
    let opts = MTLCompileOptions::new();
    let src = NSString::from_str(CURSOR_MSL);
    let lib = device
        .newLibraryWithSource_options_error(&src, Some(&opts))
        .ok()?;
    let vs = lib.newFunctionWithName(&NSString::from_str("wmt_cursor_vs"))?;
    let ps = lib.newFunctionWithName(&NSString::from_str("wmt_cursor_ps"))?;
    let desc = MTLRenderPipelineDescriptor::new();
    desc.setVertexFunction(Some(&vs));
    desc.setFragmentFunction(Some(&ps));
    // SAFETY: index 0 color attachment exists on every `MTLRenderPipelineDescriptor`.
    let color0 = unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) };
    color0.setPixelFormat(format);
    let pso = device
        .newRenderPipelineStateWithDescriptor_error(&desc)
        .ok()?;
    cache.insert(format.0, Retained::clone(&pso));
    Some(pso)
}

fn upload_sprite_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    sprite: &Sprite,
    storage: MTLStorageMode,
) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
    let desc = MTLTextureDescriptor::new();
    desc.setTextureType(MTLTextureType::Type2D);
    desc.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    // SAFETY: validated positive width.
    unsafe { desc.setWidth(sprite.width as usize) };
    // SAFETY: validated positive height.
    unsafe { desc.setHeight(sprite.height as usize) };
    desc.setUsage(MTLTextureUsage::ShaderRead);
    desc.setStorageMode(storage);
    let texture = device.newTextureWithDescriptor(&desc)?;
    let region = MTLRegion {
        origin: MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: sprite.width as usize,
            height: sprite.height as usize,
            depth: 1,
        },
    };
    let bytes_per_row = sprite.width as usize * 4;
    let pixels = NonNull::from(&*sprite.pixels).cast::<c_void>();
    // SAFETY: `sprite.pixels` holds `width * height * 4` tightly packed BGRA bytes.
    unsafe {
        texture.replaceRegion_mipmapLevel_slice_withBytes_bytesPerRow_bytesPerImage(
            region,
            0,
            0,
            pixels,
            bytes_per_row,
            sprite.pixels.len(),
        );
    }
    Some(texture)
}

const fn image_layout(format: MTLPixelFormat) -> Option<(usize, usize, CGBitmapInfo)> {
    match format {
        MTLPixelFormat::BGRA8Unorm => Some((
            4,
            8,
            CGBitmapInfo(
                CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
            ),
        )),
        MTLPixelFormat::RGBA16Float => Some((
            8,
            16,
            CGBitmapInfo(
                CGImageAlphaInfo::PremultipliedLast.0
                    | CGImageByteOrderInfo::Order16Little.0
                    | CGImageComponentInfo::Float.0,
            ),
        )),
        _ => None,
    }
}

fn transparent_image() -> Option<CFRetained<CGImage>> {
    let space = CGColorSpace::new_device_rgb()?;
    cgimage_from_pixels(1, 1, MTLPixelFormat::BGRA8Unorm, Some(&space), &[0; 4])
}

fn readback_texture(
    texture: &ProtocolObject<dyn MTLTexture>,
    space: Option<&CGColorSpace>,
) -> Option<CFRetained<CGImage>> {
    let format = texture.pixelFormat();
    let (bpp, _, _) = image_layout(format)?;
    let width = texture.width();
    let height = texture.height();
    let row = width.checked_mul(bpp)?;
    let mut pixels = vec![0u8; row.checked_mul(height)?];
    let dst = NonNull::new(pixels.as_mut_ptr())?.cast::<c_void>();
    // SAFETY: `pixels` holds `row * height` bytes matching `region` at mipmap 0.
    unsafe {
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            dst,
            row,
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width,
                    height,
                    depth: 1,
                },
            },
            0,
        );
    }
    let fallback_space = if space.is_none() {
        fallback_colorspace(format)
    } else {
        None
    };
    let effective_space = space.or(fallback_space.as_deref());
    cgimage_from_pixels(width, height, format, effective_space, &pixels)
}

fn fallback_colorspace(format: MTLPixelFormat) -> Option<CFRetained<CGColorSpace>> {
    if format == MTLPixelFormat::RGBA16Float {
        // SAFETY: process-lifetime CoreGraphics constant.
        let name = unsafe { kCGColorSpaceExtendedLinearSRGB };
        CGColorSpace::with_name(Some(name))
    } else {
        // SAFETY: process-lifetime CoreGraphics constant.
        let name = unsafe { kCGColorSpaceSRGB };
        CGColorSpace::with_name(Some(name)).or_else(CGColorSpace::new_device_rgb)
    }
}

fn cgimage_from_pixels(
    width: usize,
    height: usize,
    format: MTLPixelFormat,
    space: Option<&CGColorSpace>,
    pixels: &[u8],
) -> Option<CFRetained<CGImage>> {
    let (bpp, bits, info) = image_layout(format)?;
    let row = width.checked_mul(bpp)?;
    if width == 0 || height == 0 || row.checked_mul(height) != Some(pixels.len()) {
        return None;
    }
    let data = CFData::from_bytes(pixels);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: packed pixel buffer owned by `provider` matches `info`, `bits`, and `row`.
    unsafe {
        CGImage::new(
            width,
            height,
            bits,
            bpp * 8,
            row,
            space,
            info,
            Some(&provider),
            core::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

fn set_layer_image(layer: &CALayer, image: &CGImage) {
    // SAFETY: `CGImageRef` is accepted as `CALayer.contents` without sending NSObject messages.
    let object = unsafe { NonNull::from(image).cast::<AnyObject>().as_ref() };
    // SAFETY: `layer` retains the supplied `CGImage`.
    unsafe { layer.setContents(Some(object)) };
}

fn apply_sprite_image(layer: &CAMetalLayer, pixels: &CGImage, key: &SpriteKey) {
    layer.setBounds(CGRect {
        origin: CGPoint::default(),
        size: CGSize {
            width: key.width_pt,
            height: key.height_pt,
        },
    });
    layer.setContentsScale(key.scale);
    set_layer_image(layer, pixels);
}

fn disable_hud_on_layer(layer: &CAMetalLayer) {
    let key = NSString::from_str("mode");
    let val = NSString::from_str("disabled");
    let hud = NSDictionary::from_slices::<NSString>(&[&key], &[&*val]);
    // SAFETY: `NSDictionary<NSString, NSString>` casts losslessly to untyped `NSDictionary`.
    let hud = unsafe { Retained::cast_unchecked::<NSDictionary>(hud) };
    // SAFETY: layer copies the `developerHUDProperties` dictionary.
    unsafe { layer.setDeveloperHUDProperties(Some(&hud)) };
}

fn no_actions() -> Retained<NSDictionary<NSString, ProtocolObject<dyn CAAction>>> {
    let null = NSNull::null();
    let none: &ProtocolObject<dyn CAAction> = ProtocolObject::from_ref(&*null);
    let keys = [
        NSString::from_str("position"),
        NSString::from_str("bounds"),
        NSString::from_str("contentsScale"),
        NSString::from_str("contents"),
    ];
    NSDictionary::from_slices(
        &[&*keys[0], &*keys[1], &*keys[2], &*keys[3]],
        &[none, none, none, none],
    )
}

fn select_native_blank() {
    POINTER_WATCH.with_borrow_mut(|watch| {
        if watch.native_cursor.is_none() {
            watch.native_cursor = native_blank_cursor();
        }
        let current = NSCursor::currentCursor();
        if let Some(cursor) = watch.native_cursor.as_ref()
            && !core::ptr::eq(&raw const *current, &raw const **cursor)
        {
            cursor.set();
        }
    });
}

fn native_blank_cursor() -> Option<Retained<NSCursor>> {
    // SAFETY: process-lifetime AppKit color-space constant.
    let color_space = unsafe { NSDeviceRGBColorSpace };
    // SAFETY: null planes asks AppKit to allocate 1x1 RGBA8 backing storage.
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            core::ptr::null_mut(),
            1,
            1,
            8,
            4,
            true,
            false,
            color_space,
            4,
            32,
        )
    }?;
    bitmap.setColor_atX_y(&NSColor::clearColor(), 0, 0);
    let image = NSImage::initWithSize(
        NSImage::alloc(),
        CGSize {
            width: 1.0,
            height: 1.0,
        },
    );
    image.addRepresentation(&bitmap);
    Some(NSCursor::initWithImage_hotSpot(
        NSCursor::alloc(),
        &image,
        CGPoint { x: 0.0, y: 0.0 },
    ))
}

fn game_window(mtm: MainThreadMarker, wanted: &WantedSnapshot) -> Option<Retained<NSWindow>> {
    wanted
        .owner
        .as_ref()
        .and_then(|att| attachment::retain_view(att, mtm))
        .and_then(|view| view.window())
}
