use core::{
    cell::Cell,
    ffi::c_void,
    mem::offset_of,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
use std::sync::LazyLock;

use dxmt_util::{
    com::{
        Guid, Hresult,
        hresult::{
            DXGI_ERROR_INVALID_CALL, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
            DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, E_FAIL, E_INVALIDARG, E_NOINTERFACE, S_OK,
        },
        iid::IID_IDXGI_OUTPUT1,
    },
    dxgi_vtbl::IDXGIOutput1Vtbl,
    ext::{
        ModeDesc, ModeDesc1, OutputDesc, Rational, SampleDesc, SwapChainDesc1,
        SwapChainFullscreenDesc,
    },
    log::Logger,
    wsi::{Hmonitor, Hwnd, WindowState, WsiMode},
};

use super::{
    Host, enter_fullscreen_mode, get_containing_output, leave_fullscreen_background,
    leave_fullscreen_mode, resize_target, set_fullscreen_state,
};
use crate::swapchain::{Core, State};

static LOG: Logger = Logger::new("no_test_log/swapchain.log");

/// The window's own monitor in the fake window system.
const WINDOW_MONITOR: Hmonitor = ptr::without_provenance_mut(0x10);

thread_local! {
    static IS_WINDOW: Cell<bool> = const { Cell::new(true) };
    static FOREGROUND: Cell<bool> = const { Cell::new(true) };
    static MINIMISED: Cell<u32> = const { Cell::new(0) };
    static RESIZED: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

/// What the hooks saw.
#[derive(Default)]
struct Seen {
    changing: u32,
    left: Vec<bool>,
}

struct Fake {
    core: Core<Seen>,
}

impl Fake {
    fn new() -> Self {
        let state = State {
            desc: SwapChainDesc1 {
                width: 1280,
                height: 720,
                format: 87,
                stereo: 0,
                sample_desc: SampleDesc {
                    count: 1,
                    quality: 0,
                },
                buffer_usage: 0x20,
                buffer_count: 2,
                scaling: 0,
                swap_effect: 3,
                alpha_mode: 0,
                flags: 0,
            },
            fullscreen_desc: SwapChainFullscreenDesc {
                refresh_rate: Rational {
                    numerator: 0,
                    denominator: 0,
                },
                scanline_ordering: 0,
                scaling: 0,
                windowed: 1,
            },
            monitor: WINDOW_MONITOR,
            target: ptr::null_mut(),
            window_state: WindowState::default(),
            presentation_count: 0,
            frame_latency: 1,
            color_space: 0,
            init_refresh_rate: f64::MAX,
            api: Seen::default(),
        };
        Self {
            core: Core::new(ptr::without_provenance_mut(0x1234), 0, None, state),
        }
    }
}

impl Host for Fake {
    type Api = Seen;

    fn core(&self) -> &Core<Seen> {
        &self.core
    }

    fn adapter(&self) -> Option<*mut c_void> {
        None
    }

    fn log() -> &'static Logger {
        &LOG
    }

    unsafe fn target_changing(&self, state: &mut State<Seen>, _output: *mut c_void) {
        state.changing += 1;
    }

    fn left(&self, state: &mut State<Seen>, restored: bool) {
        state.left.push(restored);
    }

    fn window_monitor(_hwnd: Hwnd) -> Hmonitor {
        WINDOW_MONITOR
    }
    fn current_display_mode(_monitor: Hmonitor) -> Option<WsiMode> {
        None
    }
    fn is_window(_hwnd: Hwnd) -> bool {
        IS_WINDOW.get()
    }
    fn is_process_foreground() -> bool {
        FOREGROUND.get()
    }
    fn minimize_window(_hwnd: Hwnd) {
        MINIMISED.set(MINIMISED.get() + 1);
    }
    fn save_window_state(_hwnd: Hwnd, _state: &mut WindowState) {}
    fn set_window_mode(_monitor: Hmonitor, _hwnd: Hwnd, _mode: &WsiMode) -> bool {
        true
    }
    fn enter_fullscreen_mode(
        _monitor: Hmonitor,
        _hwnd: Hwnd,
        _state: &mut WindowState,
        _mode_switch: bool,
    ) -> bool {
        true
    }
    fn leave_fullscreen_mode(_hwnd: Hwnd, _state: &WindowState, _restore: bool) -> bool {
        true
    }
    fn restore_display_mode(_monitor: Hmonitor) -> bool {
        true
    }
    fn resize_window(_hwnd: Hwnd, _state: &WindowState, width: u32, height: u32) {
        RESIZED.set((width, height));
    }
    fn update_fullscreen_window(_monitor: Hmonitor, _hwnd: Hwnd, _force_topmost: bool) {}
}

/// An `IDXGIOutput1` on `monitor` whose `FindClosestMatchingMode1` answers `find_hr`, or 60 Hz.
#[repr(C)]
struct FakeOutput {
    vtbl: *const IDXGIOutput1Vtbl,
    refs: AtomicU32,
    monitor: Hmonitor,
    find_hr: Hresult,
}

impl FakeOutput {
    fn new(monitor: usize, find_hr: Hresult) -> Self {
        Self {
            vtbl: &raw const *OUTPUT_VTBL,
            refs: AtomicU32::new(1),
            monitor: ptr::without_provenance_mut(monitor),
            find_hr,
        }
    }

    fn com(&self) -> *mut c_void {
        ptr::from_ref(self).cast_mut().cast()
    }

    fn refs(&self) -> u32 {
        self.refs.load(Ordering::Acquire)
    }

    /// The pointer with a reference of its own, for a call that consumes one.
    fn add_ref(&self) -> *mut c_void {
        self.refs.fetch_add(1, Ordering::AcqRel);
        self.com()
    }
}

unsafe extern "system-unwind" fn output_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    // SAFETY: the core passes a live IID.
    let iid = unsafe { riid.cast::<Guid>().read_unaligned() };
    if iid != IID_IDXGI_OUTPUT1 {
        return E_NOINTERFACE;
    }
    // SAFETY: `this` is a live `FakeOutput`.
    unsafe { output_add_ref(this) };
    // SAFETY: the core's local.
    unsafe { out.write(this) };
    S_OK
}

unsafe extern "system-unwind" fn output_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `FakeOutput`.
    unsafe { &*this.cast::<FakeOutput>() }
        .refs
        .fetch_add(1, Ordering::AcqRel)
        + 1
}

unsafe extern "system-unwind" fn output_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `FakeOutput`; the test owns its memory.
    unsafe { &*this.cast::<FakeOutput>() }
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1
}

unsafe extern "system-unwind" fn output_get_desc(this: *mut c_void, desc: *mut c_void) -> i32 {
    // SAFETY: `this` is a live `FakeOutput`.
    let monitor = unsafe { &*this.cast::<FakeOutput>() }.monitor;
    // SAFETY: the core's local `OutputDesc`.
    unsafe { (*desc.cast::<OutputDesc>()).monitor = monitor };
    S_OK
}

unsafe extern "system-unwind" fn output_find_mode(
    this: *mut c_void,
    preferred: *const c_void,
    selected: *mut c_void,
    _device: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `FakeOutput`.
    let find_hr = unsafe { &*this.cast::<FakeOutput>() }.find_hr;
    if find_hr < 0 {
        return find_hr;
    }
    // SAFETY: the core's local preferred mode.
    let mut mode = unsafe { preferred.cast::<ModeDesc1>().read() };
    mode.refresh_rate = Rational {
        numerator: 60,
        denominator: 0,
    };
    // SAFETY: the core's local selected mode.
    unsafe { selected.cast::<ModeDesc1>().write(mode) };
    S_OK
}

const extern "system-unwind" fn unused() {}

const SLOTS: usize = size_of::<IDXGIOutput1Vtbl>() / size_of::<usize>();

static OUTPUT_VTBL: LazyLock<IDXGIOutput1Vtbl> = LazyLock::new(|| {
    let slot = |offset: usize| offset / size_of::<usize>();
    let mut slots = [unused as *const (); SLOTS];
    slots[slot(offset_of!(IDXGIOutput1Vtbl, query_interface))] = output_qi as *const ();
    slots[slot(offset_of!(IDXGIOutput1Vtbl, add_ref))] = output_add_ref as *const ();
    slots[slot(offset_of!(IDXGIOutput1Vtbl, release))] = output_release as *const ();
    slots[slot(offset_of!(IDXGIOutput1Vtbl, get_desc))] = output_get_desc as *const ();
    slots[slot(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1))] =
        output_find_mode as *const ();
    // SAFETY: every slot holds a function's address; the machine calls only the five above, each
    // with the signature its slot names.
    unsafe { core::mem::transmute::<[*const (); SLOTS], IDXGIOutput1Vtbl>(slots) }
});

#[test]
fn enter_and_leave_hold_the_target_once() {
    let host = Fake::new();
    let output = FakeOutput::new(0x20, S_OK);
    // SAFETY: a live output with a reference the call takes over.
    let hr = unsafe { enter_fullscreen_mode(&host, output.add_ref()) };
    assert_eq!(hr, S_OK);
    {
        let st = host.core.lock();
        assert_eq!(st.fullscreen_desc.windowed, 0);
        assert_eq!(st.target, output.com());
        assert_eq!(st.monitor, output.monitor);
        assert!((st.init_refresh_rate - 60.0).abs() < f64::EPSILON);
        assert_eq!(st.changing, 1);
    }
    assert_eq!(output.refs(), 2);

    FOREGROUND.set(false);
    assert_eq!(leave_fullscreen_mode(&host, false), S_OK);
    let st = host.core.lock();
    assert_eq!(st.fullscreen_desc.windowed, 1);
    assert!(st.target.is_null());
    assert_eq!(st.monitor, WINDOW_MONITOR);
    assert_eq!(st.left, [true]);
    assert_eq!(MINIMISED.get(), 1, "left in the background: minimised");
    assert_eq!(output.refs(), 1);
}

#[test]
fn refused_entries_give_the_target_back() {
    let host = Fake::new();
    let failing = FakeOutput::new(0x20, E_FAIL);
    // SAFETY: as above.
    let hr = unsafe { enter_fullscreen_mode(&host, failing.add_ref()) };
    assert_eq!(hr, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE);
    assert_eq!(failing.refs(), 1);
    assert_eq!(host.core.lock().fullscreen_desc.windowed, 1);

    let output = FakeOutput::new(0x20, S_OK);
    host.core.modeset_in_progress.store(true, Ordering::Release);
    // SAFETY: as above.
    let hr = unsafe { enter_fullscreen_mode(&host, output.add_ref()) };
    assert_eq!(hr, DXGI_STATUS_MODE_CHANGE_IN_PROGRESS);
    assert_eq!(
        leave_fullscreen_mode(&host, false),
        DXGI_STATUS_MODE_CHANGE_IN_PROGRESS
    );
    assert!(
        host.core.modeset_in_progress.load(Ordering::Acquire),
        "a refused change leaves the running one's flag"
    );
    host.core
        .modeset_in_progress
        .store(false, Ordering::Release);

    IS_WINDOW.set(false);
    // SAFETY: as above.
    let hr = unsafe { enter_fullscreen_mode(&host, output.add_ref()) };
    assert_eq!(hr, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE);
    assert_eq!(output.refs(), 1);

    IS_WINDOW.set(true);
    // No target and no adapter: the window's output cannot be found.
    // SAFETY: a null target.
    let hr = unsafe { enter_fullscreen_mode(&host, ptr::null_mut()) };
    assert_eq!(hr, E_FAIL);
}

#[test]
fn another_monitor_is_left_before_entering() {
    let host = Fake::new();
    let first = FakeOutput::new(0x20, S_OK);
    let second = FakeOutput::new(0x30, S_OK);
    // SAFETY: a live output the caller keeps.
    let hr = unsafe { set_fullscreen_state(&host, 1, first.com()) };
    assert_eq!(hr, S_OK);
    assert_eq!(first.refs(), 2);

    FOREGROUND.set(false);
    // SAFETY: as above.
    let hr = unsafe { set_fullscreen_state(&host, 1, second.com()) };
    assert_eq!(hr, S_OK);
    assert_eq!(MINIMISED.get(), 0, "switching outputs does not minimise");
    {
        let st = host.core.lock();
        assert_eq!(st.target, second.com());
        assert_eq!(st.left, [true]);
        assert_eq!(st.changing, 2);
    }
    assert_eq!((first.refs(), second.refs()), (1, 2));

    // Already fullscreen: asking again changes nothing.
    // SAFETY: as above.
    let hr = unsafe { set_fullscreen_state(&host, 1, second.com()) };
    assert_eq!(hr, S_OK);
    assert_eq!(second.refs(), 2);

    // SAFETY: a null target.
    let hr = unsafe { set_fullscreen_state(&host, 0, ptr::null_mut()) };
    assert_eq!(hr, S_OK);
    assert_eq!(second.refs(), 1);
    assert_eq!(MINIMISED.get(), 1);
}

#[test]
fn containing_output_and_resize_target() {
    let host = Fake::new();
    // SAFETY: a null `out`.
    let hr = unsafe { get_containing_output(&host, ptr::null_mut(), E_INVALIDARG) };
    assert_eq!(hr, E_INVALIDARG);
    let mut out = ptr::without_provenance_mut(1);
    // No target and no adapter.
    // SAFETY: `out` is a local.
    let hr = unsafe { get_containing_output(&host, &raw mut out, E_INVALIDARG) };
    assert_eq!((hr, out), (E_FAIL, ptr::null_mut()));

    let output = FakeOutput::new(0x20, S_OK);
    // SAFETY: as in the tests above.
    let hr = unsafe { enter_fullscreen_mode(&host, output.add_ref()) };
    assert_eq!(hr, S_OK);
    // SAFETY: `out` is a local.
    let hr = unsafe { get_containing_output(&host, &raw mut out, E_INVALIDARG) };
    assert_eq!((hr, out), (S_OK, output.com()));
    assert_eq!(output.refs(), 3);
    // SAFETY: gives back the reference `out` holds.
    unsafe { output_release(out) };

    let mode = ModeDesc {
        width: 800,
        height: 600,
        refresh_rate: Rational {
            numerator: 75,
            denominator: 1,
        },
        format: 0,
        scanline_ordering: 1,
        scaling: 2,
    };
    // SAFETY: `mode` is a local.
    let hr = unsafe { resize_target(&host, (&raw const mode).cast()) };
    assert_eq!(hr, S_OK);
    {
        let st = host.core.lock();
        assert_eq!(st.fullscreen_desc.refresh_rate.numerator, 75);
        assert_eq!(
            (
                st.fullscreen_desc.scanline_ordering,
                st.fullscreen_desc.scaling
            ),
            (1, 2)
        );
    }
    assert_eq!(
        output.refs(),
        2,
        "the mode change's reference is given back"
    );

    assert_eq!(leave_fullscreen_mode(&host, true), S_OK);
    // SAFETY: as above.
    let hr = unsafe { resize_target(&host, (&raw const mode).cast()) };
    assert_eq!(hr, S_OK);
    assert_eq!(RESIZED.get(), (800, 600));
    // SAFETY: a null mode.
    let hr = unsafe { resize_target(&host, ptr::null()) };
    assert_eq!(hr, DXGI_ERROR_INVALID_CALL);
}

#[test]
fn background_leave_minimises_unless_switching() {
    let hwnd: Hwnd = ptr::without_provenance_mut(0x1234);
    for (switching, foreground, minimised) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let logged = Cell::new(None);
        let seen = Cell::new(None);
        leave_fullscreen_background(
            hwnd,
            switching,
            || foreground,
            |line| logged.set(Some(line)),
            |w| seen.set(Some(w)),
        );
        assert_eq!(seen.get().is_some(), minimised);
        assert_eq!(
            logged.get(),
            minimised.then_some("Leaving fullscreen in the background: minimising")
        );
    }
}
