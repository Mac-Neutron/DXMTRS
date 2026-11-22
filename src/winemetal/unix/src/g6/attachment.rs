//! One attachment record per attached view: lock-free display and poll state.
//!
//! `WMTAttachMetalView` registers an [`Attachment`] keyed by the raw `NSView*` address and returns
//! both the view handle and its retained layer. Subsequent per-present polls (`WMTViewSyncAndPoll`)
//! read only atomics from this record without hopping to the main thread or touching `AppKit` view
//! or window chains off the main thread.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::{
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::NSView;
use objc2_quartz_core::CAMetalLayer;
use rustc_hash::FxHashMap;
use winemetal_wire::{
    WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY, WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED,
    WMT_VIEW_POLL_FLAG_CURSOR_KICK, WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING,
    WMT_VIEW_POLL_FLAG_HDR_ACTIVE, WMT_VIEW_POLL_FLAG_OCCLUDED,
    WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE, WMTViewPollOut,
};

/// How many polls may pass between asynchronous main-thread display refreshes.
pub const HEADROOM_REFRESH_POLLS: u64 = 32;

/// Consecutive undisplayed paced presents before a visible window counts as showing nothing.
pub const UNDISPLAYED_PACED_LIMIT: u32 = 16;

/// Software cursor policy resolved from `WMTViewAttachFlag`.
pub const CURSOR_POLICY_AUTO: u8 = 0;
pub const CURSOR_POLICY_ON: u8 = 1;
pub const CURSOR_POLICY_OFF: u8 = 2;

/// Static configuration bit: caller asked to enable HDR when panel supports EDR.
pub const STATIC_HDR_ENABLE: u8 = 1 << 0;
/// Static configuration bit: caller requested sRGB-accurate colorspace instead of passthrough.
pub const STATIC_ACCURATE_COLORSPACE: u8 = 1 << 1;

static ATTACHMENTS: LazyLock<Mutex<FxHashMap<usize, Arc<Attachment>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

fn lock_attachments() -> MutexGuard<'static, FxHashMap<usize, Arc<Attachment>>> {
    ATTACHMENTS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Arguments captured when registering a new attachment record.
pub struct AttachLatches {
    pub hwnd: u64,
    pub static_flags: u8,
    pub hdr_active: bool,
    pub software_cursor_policy: u8,
    pub backing_scale: u32,
    pub edr_current: f32,
    pub edr_potential: f32,
    pub panel_max_hz: u32,
    pub screen_id: u32,
    pub client_surface: usize,
}

/// Per-view display, HDR, occlusion, and latched dirty state.
pub struct Attachment {
    view: usize,
    layer: usize,
    hwnd: u64,
    client_surface: usize,
    client_view_watch: AtomicUsize,
    attached_at: Instant,
    static_flags: u8,
    software_cursor_policy: u8,
    window: AtomicUsize,
    screen_id: AtomicU32,
    hdr_active: AtomicBool,
    window_occluded: AtomicBool,
    current_headroom_bits: AtomicU32,
    edr_potential_bits: AtomicU32,
    panel_max_hz: AtomicU32,
    backing_scale: AtomicU32,
    headroom_refresh_pending: AtomicBool,
    polls_since_refresh: AtomicU64,
    undisplayed_paced: AtomicU32,
    client_surface_dirty: AtomicBool,
    cursor_kick: AtomicBool,
    colorspace_changed: AtomicBool,
}

impl Attachment {
    fn new(view: usize, layer: usize, latches: &AttachLatches) -> Self {
        Self {
            view,
            layer,
            hwnd: latches.hwnd,
            client_surface: latches.client_surface,
            client_view_watch: AtomicUsize::new(0),
            attached_at: Instant::now(),
            static_flags: latches.static_flags,
            software_cursor_policy: latches.software_cursor_policy,
            window: AtomicUsize::new(0),
            screen_id: AtomicU32::new(latches.screen_id),
            hdr_active: AtomicBool::new(latches.hdr_active),
            window_occluded: AtomicBool::new(false),
            current_headroom_bits: AtomicU32::new(latches.edr_current.to_bits()),
            edr_potential_bits: AtomicU32::new(latches.edr_potential.to_bits()),
            panel_max_hz: AtomicU32::new(latches.panel_max_hz),
            backing_scale: AtomicU32::new(latches.backing_scale.clamp(1, 8)),
            headroom_refresh_pending: AtomicBool::new(false),
            polls_since_refresh: AtomicU64::new(0),
            undisplayed_paced: AtomicU32::new(0),
            client_surface_dirty: AtomicBool::new(false),
            cursor_kick: AtomicBool::new(false),
            colorspace_changed: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub const fn view(&self) -> usize {
        self.view
    }

    #[must_use]
    pub const fn layer(&self) -> usize {
        self.layer
    }

    #[must_use]
    pub const fn hwnd(&self) -> u64 {
        self.hwnd
    }

    #[must_use]
    pub const fn client_surface(&self) -> usize {
        self.client_surface
    }

    #[must_use]
    pub const fn hdr_enable_requested(&self) -> bool {
        self.static_flags & STATIC_HDR_ENABLE != 0
    }

    #[must_use]
    pub const fn accurate_colorspace(&self) -> bool {
        self.static_flags & STATIC_ACCURATE_COLORSPACE != 0
    }

    pub fn set_window(&self, window: usize) {
        self.window.store(window, Ordering::Relaxed);
    }

    #[must_use]
    pub fn window(&self) -> usize {
        self.window.load(Ordering::Relaxed)
    }

    pub fn swap_screen_id(&self, next: u32) -> u32 {
        self.screen_id.swap(next, Ordering::Relaxed)
    }

    #[must_use]
    pub fn hdr_active(&self) -> bool {
        self.hdr_active.load(Ordering::Relaxed)
    }

    pub fn set_hdr_active(&self, active: bool) {
        let prev = self.hdr_active.swap(active, Ordering::AcqRel);
        if prev != active {
            self.colorspace_changed.store(true, Ordering::Release);
        }
    }

    pub fn mark_colorspace_changed(&self) {
        self.colorspace_changed.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn software_cursor_active(&self) -> bool {
        match self.software_cursor_policy {
            CURSOR_POLICY_ON => true,
            CURSOR_POLICY_OFF => false,
            _ => self.hdr_active(),
        }
    }

    pub fn set_window_occluded(&self, occluded: bool) {
        self.window_occluded.store(occluded, Ordering::Relaxed);
    }

    #[must_use]
    pub fn window_occluded(&self) -> bool {
        self.window_occluded.load(Ordering::Relaxed)
    }

    pub fn set_headroom(&self, headroom: f32) {
        self.current_headroom_bits
            .store(headroom.to_bits(), Ordering::Relaxed);
    }

    #[must_use]
    pub fn headroom(&self) -> f32 {
        f32::from_bits(self.current_headroom_bits.load(Ordering::Relaxed))
    }

    pub fn set_edr_potential(&self, potential: f32) {
        self.edr_potential_bits
            .store(potential.to_bits(), Ordering::Relaxed);
    }

    pub fn set_panel_max_hz(&self, hz: u32) {
        self.panel_max_hz.store(hz, Ordering::Relaxed);
    }

    pub fn set_backing_scale(&self, scale: u32) {
        self.backing_scale
            .store(scale.clamp(1, 8), Ordering::Relaxed);
    }

    #[must_use]
    pub fn backing_scale(&self) -> u32 {
        self.backing_scale.load(Ordering::Relaxed).clamp(1, 8)
    }

    pub fn replace_client_view_watch(&self, watch: usize) -> usize {
        self.client_view_watch.swap(watch, Ordering::AcqRel)
    }

    pub fn take_client_view_watch(&self) -> usize {
        self.client_view_watch.swap(0, Ordering::AcqRel)
    }

    pub fn set_client_surface_dirty(&self) {
        self.client_surface_dirty.store(true, Ordering::Release);
    }

    pub fn set_cursor_kick(&self) {
        self.cursor_kick.store(true, Ordering::Release);
    }

    /// Record whether one paced present was shown, returning whether `DISPLAYS_NOTHING` moved.
    pub fn note_paced_present(&self, displayed: bool) -> Option<bool> {
        if displayed {
            let prev = self.undisplayed_paced.swap(0, Ordering::Relaxed);
            return (prev >= UNDISPLAYED_PACED_LIMIT).then_some(false);
        }
        if self.window_occluded() {
            return None;
        }
        let mut cur = self.undisplayed_paced.load(Ordering::Relaxed);
        loop {
            if cur >= UNDISPLAYED_PACED_LIMIT {
                return None;
            }
            let next = cur + 1;
            match self.undisplayed_paced.compare_exchange_weak(
                cur,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return (next == UNDISPLAYED_PACED_LIMIT).then_some(true),
                Err(actual) => cur = actual,
            }
        }
    }

    #[must_use]
    pub fn displays_nothing(&self) -> bool {
        self.undisplayed_paced.load(Ordering::Relaxed) >= UNDISPLAYED_PACED_LIMIT
    }

    /// Ask whether a periodic main-thread refresh is due, claiming the one-in-flight slot.
    pub fn claim_refresh_if_due(&self, force: bool) -> bool {
        let count = self.polls_since_refresh.fetch_add(1, Ordering::Relaxed) + 1;
        if !force && count < HEADROOM_REFRESH_POLLS {
            return false;
        }
        if self
            .headroom_refresh_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.polls_since_refresh.store(0, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn end_refresh(&self) {
        self.headroom_refresh_pending
            .store(false, Ordering::Release);
    }

    /// Read lock-free display state and consume latched edge flags into `out`.
    pub fn write_poll_out(
        &self,
        drawable_width: u32,
        drawable_height: u32,
        out: &mut WMTViewPollOut,
    ) {
        let mut flags = 0u32;
        if self.hdr_active() {
            flags |= WMT_VIEW_POLL_FLAG_HDR_ACTIVE;
        }
        if self.software_cursor_active() {
            flags |= WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE;
        }
        if self.window_occluded() {
            flags |= WMT_VIEW_POLL_FLAG_OCCLUDED;
        }
        if self.displays_nothing() {
            flags |= WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING;
        }
        if self.client_surface_dirty.swap(false, Ordering::AcqRel) {
            flags |= WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY;
        }
        if self.cursor_kick.swap(false, Ordering::AcqRel) {
            flags |= WMT_VIEW_POLL_FLAG_CURSOR_KICK;
        }
        if self.colorspace_changed.swap(false, Ordering::AcqRel) {
            flags |= WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED;
        }
        out.drawable_width = drawable_width;
        out.drawable_height = drawable_height;
        out.backing_scale = self.backing_scale();
        out.edr_current = self.headroom();
        out.edr_potential = f32::from_bits(self.edr_potential_bits.load(Ordering::Relaxed));
        out.panel_max_hz = self.panel_max_hz.load(Ordering::Relaxed);
        out.flags = flags;
        out.reserved = 0;
    }
}

/// Register a view attachment record and return any displaced stale record for that view address.
#[must_use]
pub fn register(
    view: usize,
    layer: usize,
    latches: &AttachLatches,
) -> (Arc<Attachment>, Option<Arc<Attachment>>) {
    let record = Arc::new(Attachment::new(view, layer, latches));
    let mut map = lock_attachments();
    let prev = map.insert(view, Arc::clone(&record));
    drop(map);
    (record, prev)
}

/// Unregister `view` and return its record if present.
#[must_use]
pub fn unregister(view: usize) -> Option<Arc<Attachment>> {
    let mut map = lock_attachments();
    map.remove(&view)
}

/// Look up a live attachment record by `view` address.
#[must_use]
pub fn find(view: usize) -> Option<Arc<Attachment>> {
    let map = lock_attachments();
    map.get(&view).map(Arc::clone)
}

/// Snapshot all currently live attachment records.
#[must_use]
pub fn live() -> Vec<Arc<Attachment>> {
    let map = lock_attachments();
    map.values().map(Arc::clone).collect()
}

/// Retain the newest live client surface on `hwnd` while holding the registry lock.
pub fn retain_newest_surface_on(
    hwnd: u64,
    retain_fn: impl FnOnce(usize) -> usize,
) -> Option<usize> {
    if hwnd == 0 {
        return None;
    }
    let map = lock_attachments();
    let newest = map
        .values()
        .filter(|att| att.hwnd == hwnd && att.client_surface != 0)
        .max_by_key(|att| att.attached_at)?;
    let retained = retain_fn(newest.client_surface);
    drop(map);
    (retained != 0).then_some(retained)
}

/// Set the client-surface dirty latch on `view` if the record is still live.
#[must_use]
pub fn request_client_surface(view: usize) -> bool {
    let map = lock_attachments();
    map.get(&view).is_some_and(|att| {
        att.set_client_surface_dirty();
        true
    })
}

/// Set the cursor-kick latch on every live attachment.
pub fn request_cursor_kick_all() {
    let map = lock_attachments();
    for att in map.values() {
        att.set_cursor_kick();
    }
}

/// Retain `att`'s `NSView` only if `att` is still the live record registered for its view key.
#[must_use]
pub fn retain_view(att: &Arc<Attachment>, _mtm: MainThreadMarker) -> Option<Retained<NSView>> {
    let map = lock_attachments();
    let current = map.get(&att.view)?;
    if !Arc::ptr_eq(current, att) {
        return None;
    }
    // SAFETY: `current` is live in the map, so its view has not been retired or released yet.
    let retained = unsafe { Retained::retain(current.view as *mut NSView) };
    drop(map);
    retained
}

/// Retain `att`'s `CAMetalLayer` only if `att` is still live in the registry.
#[must_use]
pub fn retain_layer(
    att: &Arc<Attachment>,
    _mtm: MainThreadMarker,
) -> Option<Retained<CAMetalLayer>> {
    let map = lock_attachments();
    let current = map.get(&att.view)?;
    if !Arc::ptr_eq(current, att) {
        return None;
    }
    // SAFETY: `current` is live in the map, so its view keeps its `CAMetalLayer` alive.
    let retained = unsafe { Retained::retain(current.layer as *mut CAMetalLayer) };
    drop(map);
    retained
}
