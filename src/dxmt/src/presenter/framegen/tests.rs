use std::{
    cell::{Cell, RefCell},
    fmt::Write as _,
    rc::Rc,
    sync::{Mutex, MutexGuard, PoisonError},
    thread,
    time::{Duration, Instant},
};

use rustc_hash::FxHashMap;
use winemetal_sys::{
    Obj,
    wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMTFXFrameInterpolatorInfo, WMTFXTemporalScalerInfo,
        WMTTextureInfo,
    },
};

use super::{
    Backend, EnqueueArgs, FrameGen, InterpolateArgs, Order, PresentArgs, PresentMetadata,
    TemporalScaler, WmtFxTemporalScalerProps, record_upscale,
};

const DEVICE_HANDLE: u64 = 0x1000;
const CMDBUF_HANDLE: u64 = 0x1001;
const FRAME_HANDLE: u64 = 0x1002;
const DEPTH_HANDLE: u64 = 0x1003;
const MOTION_HANDLE: u64 = 0x1004;
const SCALER_HANDLE: u64 = 0x1005;
const FENCE_HANDLE: u64 = 0x1006;

static SLOT_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static STDERR_LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn capture_stderr(line: &str) {
    STDERR_LINES.with(|lines| lines.borrow_mut().push(line.to_owned()));
}

fn take_stderr() -> Vec<String> {
    STDERR_LINES.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
}

fn slot_lock() -> MutexGuard<'static, ()> {
    SLOT_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[unsafe(no_mangle)]
pub extern "C" fn MTLDevice_newTemporalScaler(
    _device: u64,
    _info: *const WMTFXTemporalScalerInfo,
) -> u64 {
    SCALER_HANDLE
}

#[unsafe(no_mangle)]
pub extern "C" fn MTLDevice_newFence(_device: u64) -> u64 {
    FENCE_HANDLE
}

fn obj(handle: u64) -> Obj {
    // SAFETY: `handle` is non-zero and `presenter::tests` provides no-op retain/release fakes.
    unsafe { Obj::adopt(handle) }.expect("non-zero handle")
}

fn scaler_1280_to_1920() -> TemporalScaler {
    let info = WMTFXTemporalScalerInfo {
        color_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        output_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        depth_format: 252,
        motion_format: 65,
        input_width: 1280,
        input_height: 720,
        output_width: 1920,
        output_height: 1080,
        input_content_min_scale: 1.0,
        input_content_max_scale: 1.0,
        auto_exposure: 0,
        input_content_properties_enabled: 0,
        requires_synchronous_initialization: 0,
        reactive_mask_enabled: 0,
        reactive_mask_format: 0,
        output_resolution_motion_vectors: 0,
        jittered_motion_vectors: 0,
    };
    // SAFETY: `MTLDevice_newTemporalScaler` and `MTLDevice_newFence` are faked above.
    unsafe { TemporalScaler::new(DEVICE_HANDLE, info) }
}

const fn default_props() -> WmtFxTemporalScalerProps {
    WmtFxTemporalScalerProps {
        input_content_width: 1280,
        input_content_height: 720,
        reset: 0,
        depth_reversed: 1,
        motion_vector_scale_x: -1280.0,
        motion_vector_scale_y: -720.0,
        jitter_offset_x: 0.25,
        jitter_offset_y: -0.375,
        pre_exposure: 1.0,
        color_content_offset_x: 0,
        color_content_offset_y: 0,
        depth_content_offset_x: 0,
        depth_content_offset_y: 0,
        motion_content_offset_x: 0,
        motion_content_offset_y: 0,
        reactive_mask_content_offset_x: 0,
        reactive_mask_content_offset_y: 0,
        output_offset_x: 0,
        output_offset_y: 0,
    }
}

const fn default_metadata() -> PresentMetadata {
    PresentMetadata {
        edr_scale: 1.0,
        max_content_luminance: 80.0,
        max_display_luminance: 80.0,
    }
}

struct RecordingBackend {
    next_handle: u64,
    names: FxHashMap<u64, String>,
    log: RcLog,
    generated_frames: Vec<(u64, bool)>,
}

struct RcLog(Rc<RefCell<String>>);

impl RcLog {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(String::new())))
    }

    fn share(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    fn line(&self, s: &str) {
        let mut buf = self.0.borrow_mut();
        buf.push_str(s);
        buf.push('\n');
    }

    fn text(&self) -> String {
        self.0.borrow().clone()
    }
}

impl RecordingBackend {
    fn new(log: RcLog) -> Self {
        let mut names = FxHashMap::default();
        names.insert(CMDBUF_HANDLE, "cmdbuf".to_owned());
        names.insert(FRAME_HANDLE, "frame".to_owned());
        names.insert(DEPTH_HANDLE, "depth".to_owned());
        names.insert(MOTION_HANDLE, "motion".to_owned());
        Self {
            next_handle: 0x2000,
            names,
            log,
            generated_frames: Vec::new(),
        }
    }

    fn alloc(&mut self, name: impl Into<String>) -> Obj {
        let h = self.next_handle;
        self.next_handle += 1;
        self.names.insert(h, name.into());
        obj(h)
    }

    fn name(&self, o: &Obj) -> &str {
        self.names
            .get(&o.handle())
            .map_or("unknown", String::as_str)
    }
}

impl Backend for RecordingBackend {
    fn texture_size_format(&mut self, tex: &Obj) -> (u64, u64, u32) {
        match tex.handle() {
            DEPTH_HANDLE => (1280, 720, 252),
            MOTION_HANDLE => (1280, 720, 65),
            _ => (1920, 1080, WMT_PIXEL_FORMAT_BGRA8_UNORM),
        }
    }

    fn new_texture(
        &mut self,
        _device: u64,
        _info: &mut WMTTextureInfo,
        generated: bool,
        slot: usize,
    ) -> Obj {
        let kind = if generated { "generated" } else { "real" };
        self.alloc(format!("fg.{kind}[{slot}]"))
    }

    fn new_command_queue(&mut self, _device: u64, _max_count: u64) -> Obj {
        self.alloc("fg.queue")
    }

    fn new_event(&mut self, _device: u64, is_written: bool) -> Obj {
        self.alloc(if is_written { "fg.written" } else { "fg.read" })
    }

    fn new_fence(&mut self, _device: u64) -> Obj {
        self.alloc("fg.fence")
    }

    fn new_frame_interpolator(
        &mut self,
        _device: u64,
        _info: &WMTFXFrameInterpolatorInfo,
    ) -> Option<Obj> {
        Some(self.alloc("fg.interpolator"))
    }

    fn spawn_pacer(&mut self, fg: &mut FrameGen) {
        fg.mark_pacer_started();
    }

    fn wait_for_pacer(&mut self, fg: &FrameGen, n: u64) -> f64 {
        let _ = fg.committed();
        self.log.line(&format!("pacing.wait(committed + 2 > {n})"));
        0.0
    }

    fn blit_encoder(&mut self, cmdbuf: &Obj, is_begin: bool) -> Obj {
        let label = if is_begin { "begin" } else { "end" };
        let cmdbuf_name = self.name(cmdbuf).to_owned();
        self.log
            .line(&format!("{label} = {cmdbuf_name}.blitCommandEncoder()"));
        self.alloc(label)
    }

    fn blit_wait_fence(&mut self, enc: &Obj, fence: &Obj) {
        self.log.line(&format!(
            "{}.waitForFence({})",
            self.name(enc),
            self.name(fence)
        ));
    }

    fn blit_update_fence(&mut self, enc: &Obj, fence: &Obj) {
        self.log.line(&format!(
            "{}.updateFence({})",
            self.name(enc),
            self.name(fence)
        ));
    }

    fn blit_copy_texture(&mut self, enc: &Obj, src: &Obj, dst: &Obj) {
        self.log.line(&format!(
            "{}.encodeCommands(copy_texture(src={}, dst={}))",
            self.name(enc),
            self.name(src),
            self.name(dst)
        ));
    }

    fn end_encoding(&mut self, enc: &Obj) {
        self.log.line(&format!("{}.endEncoding()", self.name(enc)));
    }

    fn wait_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64) {
        self.log.line(&format!(
            "{}.encodeWaitForEvent({}, {value})",
            self.name(cmdbuf),
            self.name(event)
        ));
    }

    fn signal_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64) {
        self.log.line(&format!(
            "{}.encodeSignalEvent({}, {value})",
            self.name(cmdbuf),
            self.name(event)
        ));
    }

    fn encode_interpolate(&mut self, args: &InterpolateArgs<'_>) {
        let p = args.props;
        self.log.line(&format!(
            "{}.encodeFrameInterpolate(interpolator={}, color={}, prev={}, depth={}, motion={}, \
             output={}, fence={}, props={{motion_vector_scale_x={:.3}, \
             motion_vector_scale_y={:.3}, delta_time={:.3}, near_plane={:.3}, far_plane={:.3}, \
             field_of_view={:.3}, aspect_ratio={:.3}, jitter_offset_x={:.3}, \
             jitter_offset_y={:.3}, depth_reversed={}}})",
            self.name(args.cmdbuf),
            self.name(args.interpolator),
            self.name(args.color),
            self.name(args.prev),
            self.name(args.depth),
            self.name(args.motion),
            self.name(args.output),
            self.name(args.fence),
            p.motion_vector_scale_x,
            p.motion_vector_scale_y,
            p.delta_time,
            p.near_plane,
            p.far_plane,
            p.field_of_view,
            p.aspect_ratio,
            p.jitter_offset_x,
            p.jitter_offset_y,
            p.depth_reversed,
        ));
    }

    fn enqueue(&mut self, _fg: &FrameGen, args: &EnqueueArgs<'_>) {
        let generated = args.gen_tex.is_some();
        self.generated_frames.push((args.n, generated));
        self.log.line(&format!(
            "pending.push(index={}, generated={generated})",
            args.n
        ));
    }
}

struct RecordingOrder {
    log: RcLog,
    waited: Cell<bool>,
    updated: Cell<bool>,
}

impl RecordingOrder {
    fn new(log: RcLog) -> Self {
        Self {
            log,
            waited: Cell::new(false),
            updated: Cell::new(false),
        }
    }
}

impl Order for RecordingOrder {
    fn wait(&mut self, _begin: &Obj) {
        self.waited.set(true);
        self.log
            .line("order.wait(begin): Order(nullptr, true) -> done=fences[done]");
        self.log.line("begin.waitForFence(fences[wait_0])");
        self.log.line("begin.waitForFence(fences[wait_1])");
        self.log
            .line("begin.encodeCommands(fillbuffer(buffer=scratch, length=4))");
    }

    fn update(&mut self, _end: &Obj) {
        self.updated.set(true);
        self.log
            .line("order.update(end): end.updateFence(fences[done])");
    }
}

#[test]
fn adapt_replays_cpp_trace() {
    let _ = take_stderr();
    let trace = include_str!("golden/adapt.trace");
    let expected = include_str!("golden/adapt.txt");

    let mut fg = FrameGen::new();
    let t0 = Instant::now();
    let mut out = String::new();

    for raw in trace.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let now_ms: u64 = parts.next().expect("now_ms").parse().expect("u64");
        let interval: f64 = parts.next().expect("interval").parse().expect("f64");
        let refresh: f64 = parts.next().expect("refresh").parse().expect("f64");
        let pair_in: f64 = parts.next().expect("pair").parse().expect("f64");
        let wait_in: f64 = parts.next().expect("wait").parse().expect("f64");
        fg.set_pair(pair_in);
        fg.set_wait(wait_in);
        let now = t0 + Duration::from_millis(now_ms);
        let on = fg.adapt_frame_gen(interval, refresh, now);
        for err in take_stderr() {
            out.push_str(&err);
            out.push('\n');
        }
        let learned_at_ms = fg
            .learned_at()
            .map_or(0, |t| t.duration_since(t0).as_millis());
        let holding_since_ms = fg
            .holding_since()
            .map_or(0, |t| t.duration_since(t0).as_millis());
        let _ = writeln!(
            out,
            "t={now_ms} decision={} wait={:.6} learned_cap={:.6} learned_at={learned_at_ms} \
             holding_since={holding_since_ms}",
            i32::from(on),
            fg.wait(),
            fg.learned_cap(),
        );
    }

    assert_eq!(out, expected);
}

#[test]
fn turns_on_after_half_a_second_slow() {
    let _ = take_stderr();
    let mut fg = FrameGen::new();
    let mut now = Instant::now();
    // At 60 Hz, cap = 2 / 60 = 33.33 ms, 1.1 * cap = 36.67 ms.
    // 40 ms interval exceeds 1.1 * cap.
    now += Duration::from_millis(100);
    assert!(!fg.adapt_frame_gen(0.040, 60.0, now));
    now += Duration::from_millis(400);
    assert!(!fg.adapt_frame_gen(0.040, 60.0, now));
    // A fast frame (30 ms <= 36.67 ms) resets holding_since.
    now += Duration::from_millis(50);
    assert!(!fg.adapt_frame_gen(0.030, 60.0, now));
    // Must hold slow for a full 500 ms from the new start.
    now += Duration::from_millis(100);
    assert!(!fg.adapt_frame_gen(0.040, 60.0, now));
    now += Duration::from_millis(499);
    assert!(!fg.adapt_frame_gen(0.040, 60.0, now));
    now += Duration::from_millis(1);
    assert!(fg.adapt_frame_gen(0.040, 60.0, now));
}

#[test]
fn turns_off_when_held_back() {
    let _ = take_stderr();
    let mut fg = FrameGen::new();
    let mut now = Instant::now();
    // Turn on at 120 Hz with 20 ms interval (cap = 16.67 ms, 1.1 * cap = 18.33 ms).
    now += Duration::from_millis(100);
    assert!(!fg.adapt_frame_gen(0.020, 120.0, now));
    now += Duration::from_millis(500);
    assert!(fg.adapt_frame_gen(0.020, 120.0, now));

    // Pacing wait > 0.1 * interval (0.003 > 0.0025) for 500 ms turns off and records learned_cap.
    fg.set_wait(0.003);
    now += Duration::from_millis(100);
    assert!(fg.adapt_frame_gen(0.025, 120.0, now));
    now += Duration::from_millis(499);
    assert!(fg.adapt_frame_gen(0.025, 120.0, now));
    now += Duration::from_millis(1);
    assert!(!fg.adapt_frame_gen(0.025, 120.0, now));
    assert!((fg.learned_cap() - 0.025).abs() < 1e-9);
    assert!(fg.wait().abs() < 1e-9);
}

#[test]
fn learned_cap_forgotten_after_60s() {
    let _ = take_stderr();
    let mut fg = FrameGen::new();
    let mut now = Instant::now();
    now += Duration::from_millis(100);
    assert!(!fg.adapt_frame_gen(0.020, 120.0, now));
    now += Duration::from_millis(500);
    assert!(fg.adapt_frame_gen(0.020, 120.0, now));

    fg.set_wait(0.004);
    now += Duration::from_millis(100);
    assert!(fg.adapt_frame_gen(0.030, 120.0, now));
    now += Duration::from_millis(500);
    assert!(!fg.adapt_frame_gen(0.030, 120.0, now));
    assert!((fg.learned_cap() - 0.030).abs() < 1e-9);

    // Before 60 s: 25 ms <= 1.1 * 30 ms (33 ms), so it stays off even after 600 ms.
    now += Duration::from_secs(30);
    assert!(!fg.adapt_frame_gen(0.025, 120.0, now));
    now += Duration::from_millis(600);
    assert!(!fg.adapt_frame_gen(0.025, 120.0, now));
    assert!((fg.learned_cap() - 0.030).abs() < 1e-9);

    // At >= 60 s since learned_at: learned_cap is cleared to 0, so 25 ms > 18.33 ms turns on.
    now += Duration::from_secs(30);
    assert!(!fg.adapt_frame_gen(0.025, 120.0, now));
    assert!(fg.learned_cap().abs() < 1e-9);
    now += Duration::from_millis(500);
    assert!(fg.adapt_frame_gen(0.025, 120.0, now));
}

#[test]
fn present_orders_as_cpp() {
    let _guard = slot_lock();
    let _ = take_stderr();
    let expected = include_str!("golden/present.txt");

    let cmdbuf = obj(CMDBUF_HANDLE);
    let frame = obj(FRAME_HANDLE);
    let depth = obj(DEPTH_HANDLE);
    let motion = obj(MOTION_HANDLE);
    let scaler = scaler_1280_to_1920();
    let props = default_props();
    let metadata = default_metadata();

    let log = RcLog::new();
    let mut backend = RecordingBackend::new(log.share());
    let mut fg = FrameGen::new();
    fg.set_always(true);

    let mut now = Instant::now();

    // Frame n=0: first present at 1920x1080 creates the ring; has_prev is false so generate=false.
    record_upscale(&scaler, &depth, &motion, &props);
    log.line("=== frame n=0 generate=false (1920x1080) ===");
    let mut order0 = RecordingOrder::new(log.share());
    let ok0 = fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order0,
    );
    assert!(ok0);

    // Advance through frames 1 and 2 quietly so the next frame is n=3 (slot 0, prev 2) with
    // smoothed interval = 0.020 s.
    let quiet = RcLog::new();
    let mut quiet_order = RecordingOrder::new(quiet);
    for _ in 1..=2 {
        now += Duration::from_millis(20);
        backend.log = RcLog::new();
        let ok = fg.present_with(
            &mut backend,
            &PresentArgs {
                now,
                cmdbuf: &cmdbuf,
                presenter: None,
                device: DEVICE_HANDLE,
                frame: &frame,
                metadata: &metadata,
                refresh: 120.0,
            },
            &mut quiet_order,
        );
        assert!(ok);
        fg.set_committed(fg.enqueued());
    }

    // Frame n=3: fresh upscale recorded, has_prev=true, interval=0.020 -> generate=true.
    backend.log = log.share();
    now += Duration::from_millis(20);
    record_upscale(&scaler, &depth, &motion, &props);
    log.line(
        "=== frame n=3 generate=true (1920x1080, interval=0.020, \
         mv_scale=(-1280.0,-720.0), jitter=(0.25,-0.375), depth_reversed=1) ===",
    );
    let mut order3 = RecordingOrder::new(log.share());
    let ok3 = fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order3,
    );
    assert!(ok3);

    assert_eq!(log.text(), expected);
}

#[test]
fn pacing_waits_read_before_rewrite() {
    let _guard = slot_lock();
    let _ = take_stderr();
    let cmdbuf = obj(CMDBUF_HANDLE);
    let frame = obj(FRAME_HANDLE);
    let depth = obj(DEPTH_HANDLE);
    let motion = obj(MOTION_HANDLE);
    let scaler = scaler_1280_to_1920();
    let props = default_props();
    let metadata = default_metadata();

    let log = RcLog::new();
    let mut backend = RecordingBackend::new(log);
    let mut fg = FrameGen::new();
    fg.set_always(true);
    let mut now = Instant::now();

    // Frames 0, 1, 2 fill the three ring slots without waiting on fg.read.
    for i in 0..3 {
        backend.log = RcLog::new();
        record_upscale(&scaler, &depth, &motion, &props);
        now += Duration::from_millis(20);
        let mut order = RecordingOrder::new(backend.log.share());
        assert!(fg.present_with(
            &mut backend,
            &PresentArgs {
                now,
                cmdbuf: &cmdbuf,
                presenter: None,
                device: DEVICE_HANDLE,
                frame: &frame,
                metadata: &metadata,
                refresh: 120.0,
            },
            &mut order,
        ));
        assert!(
            !backend.log.text().contains("encodeWaitForEvent(fg.read"),
            "frame {i} should not wait on fg.read before the ring wraps"
        );
        fg.set_committed(fg.enqueued());
    }

    // Frame 3 wraps to slot 0 and must encode waitForEvent(fg.read, 1) before interpolation and
    // copy_texture overwrite slot 0.
    let frame3_log = RcLog::new();
    backend.log = frame3_log.share();
    record_upscale(&scaler, &depth, &motion, &props);
    now += Duration::from_millis(20);
    let mut order3 = RecordingOrder::new(frame3_log.share());
    assert!(fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order3,
    ));

    let text = frame3_log.text();
    let wait_pos = text
        .find("cmdbuf.encodeWaitForEvent(fg.read, 1)")
        .expect("frame 3 must wait on fg.read=1");
    let interp_pos = text
        .find("cmdbuf.encodeFrameInterpolate")
        .expect("frame 3 must encode interpolation");
    let copy_pos = text
        .find("end.encodeCommands(copy_texture(src=frame, dst=fg.real[0]))")
        .expect("frame 3 must copy into fg.real[0]");
    assert!(wait_pos < interp_pos);
    assert!(interp_pos < copy_pos);
}

#[test]
fn slot_fresh_once_per_upscale() {
    let _guard = slot_lock();
    let _ = take_stderr();
    let cmdbuf = obj(CMDBUF_HANDLE);
    let frame = obj(FRAME_HANDLE);
    let depth = obj(DEPTH_HANDLE);
    let motion = obj(MOTION_HANDLE);
    let scaler = scaler_1280_to_1920();
    let props = default_props();
    let metadata = default_metadata();

    let log = RcLog::new();
    let mut backend = RecordingBackend::new(log.share());
    let mut fg = FrameGen::new();
    fg.set_always(true);
    let mut now = Instant::now();

    // Frame 0 initializes the ring and sets has_prev = true.
    record_upscale(&scaler, &depth, &motion, &props);
    let mut order = RecordingOrder::new(log);
    assert!(fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order,
    ));
    fg.set_committed(fg.enqueued());

    // Record one upscale, then present twice: the first consumes fresh and generates; the second
    // sees fresh == false and does not generate.
    record_upscale(&scaler, &depth, &motion, &props);
    now += Duration::from_millis(20);
    assert!(fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order,
    ));
    fg.set_committed(fg.enqueued());

    now += Duration::from_millis(20);
    assert!(fg.present_with(
        &mut backend,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order,
    ));

    assert_eq!(
        backend.generated_frames,
        vec![(0, false), (1, true), (2, false)]
    );
}

#[test]
fn vsync_present_not_generated() {
    let _guard = slot_lock();
    let _ = take_stderr();
    let cmdbuf = obj(CMDBUF_HANDLE);
    let frame = obj(FRAME_HANDLE);
    let depth = obj(DEPTH_HANDLE);
    let motion = obj(MOTION_HANDLE);
    let scaler = scaler_1280_to_1920();
    let props = default_props();
    let metadata = default_metadata();

    let log = RcLog::new();
    let mut backend = RecordingBackend::new(log.share());
    let mut fg = FrameGen::new();
    fg.set_always(true);
    let mut now = Instant::now();

    // Frame 0 with after == 0.0 is handed to the pacing thread (enqueued = 1, committed = 0).
    record_upscale(&scaler, &depth, &motion, &props);
    let mut order = RecordingOrder::new(log);
    let handed0 = fg.present_or_drain(
        &mut backend,
        0.0,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 120.0,
        },
        &mut order,
    );
    assert!(handed0);
    assert!(fg.running());
    assert_eq!(fg.enqueued(), 1);
    assert_eq!(fg.committed(), 0);

    // Simulate the pacing thread finishing frame 0 so `drain` wakes up, then present with vsync
    // (`after > 0.0`): bypasses `present` and drains the in-flight frame first.
    fg.set_committed(1);
    now += Duration::from_millis(16);
    record_upscale(&scaler, &depth, &motion, &props);
    let handed1 = fg.present_or_drain(
        &mut backend,
        1.0 / 60.0,
        &PresentArgs {
            now,
            cmdbuf: &cmdbuf,
            presenter: None,
            device: DEVICE_HANDLE,
            frame: &frame,
            metadata: &metadata,
            refresh: 60.0,
        },
        &mut order,
    );
    assert!(!handed1);
    assert_eq!(fg.enqueued(), 1);
    assert_eq!(fg.committed(), 1);
    let _ = thread::yield_now;
}
