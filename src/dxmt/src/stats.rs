//! `DXMT_STATS`: per-thread counters and timed calls, and the `stats.txt` report.
//!
//! `d3d12_stats.cpp` in Rust. Every call site registers a name once and keeps its id; a timed call
//! adds its counter ticks to its thread's slot, a counter (a name starting with `#`) its count.
//! Every 5 s of presents, and once at exit, the report appends to `<dump folder>/stats.txt`: per
//! thread the time spent in outermost calls and the 30 calls that took longest, then the counters
//! summed by name. The counter is the CPU's, calibrated against the clock at each report. The
//! C++ keeps `g_stats_on` (`DXMT_STATS=1`) and passes it to `dxmt_stats_init` at load. The
//! performance build has none of it.
#![cfg(not(feature = "perf-build"))]

use std::{
    cell::Cell,
    cmp::Reverse,
    collections::BTreeMap,
    ffi::c_char,
    sync::{
        LazyLock, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};

use dxmt_util::float_cast::u64_to_f64;

use crate::ffi::c_str;

mod abi;
mod platform;

/// The ids there are; names registered past the last share it.
const MAX_STATS: usize = 1024;
/// How many timed lines a thread's block prints: those that took longest.
const TOP: usize = 30;
/// How much of a piece the C++'s 512-byte `snprintf` buffer keeps.
const LINE_MAX: usize = 511;
/// How often presents report.
const PERIOD: Duration = Duration::from_secs(5);

/// Whether `DXMT_STATS` is on, set at load by [`dxmt_stats_init`] from the C++'s `g_stats_on`.
///
/// A static: a process-wide fact, latched once at load and only read after.
static ON: AtomicBool = AtomicBool::new(false);

/// The encoders numbered since the last present (`g_pass`).
///
/// A static: the process-wide resource is the numbering that `passes.txt` and the Metal encoder
/// labels share across every queue, as the C++'s global.
static PASS: AtomicU32 = AtomicU32::new(0);

/// The names, the threads, the clocks and the report text of the process's one `stats.txt`.
///
/// A static: the process-wide resource is that file, whose report sums every device's calls on
/// every thread (the C++'s globals).
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::new()));

thread_local! {
    /// The calling thread's counters, made and registered at its first call (`t_stats`).
    ///
    /// A thread local: the caller has no object to reach, since any D3D12 call on any thread
    /// records into its thread's counters. Leaked as the C++'s `new ThreadStats`; no destructor.
    static MINE: Cell<Option<&'static ThreadStats>> = const { Cell::new(None) };
}

/// A registered name's slot.
#[derive(Clone, Copy)]
pub struct StatId(u32);

/// Times one call from `new` to its drop, when statistics are on (`DXMT_STAT_SCOPE`).
pub struct Scope {
    id: StatId,
    t0: u64,
}

impl Scope {
    /// Starts timing `id`.
    #[must_use]
    pub fn new(id: StatId) -> Self {
        Self {
            id,
            t0: if on() { begin() } else { 0 },
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if on() {
            end(self.id, self.t0);
        }
    }
}

/// Whether `DXMT_STATS` is on.
#[must_use]
pub fn on() -> bool {
    ON.load(Relaxed)
}

/// Registers `name` (copied); a call site keeps the id.
#[must_use]
pub fn id(name: &str) -> StatId {
    registry().id(name)
}

/// Adds `n` to the counter `id` on the calling thread.
pub fn count(id: StatId, n: u64) {
    mine().count(id, n);
}

/// Starts a timed call on the calling thread: its counter value, for [`end`].
#[must_use]
pub fn begin() -> u64 {
    mine().enter();
    platform::counter()
}

/// Ends the timed call `id` that [`begin`] started at `t0`.
pub fn end(id: StatId, t0: u64) {
    let ticks = platform::counter().wrapping_sub(t0);
    mine().record(id, ticks);
}

/// Counts a present, and reports when 5 s have passed since the last report.
pub fn frame() {
    if !on() {
        return;
    }
    PASS.store(0, Relaxed);
    let mut registry = registry();
    if registry.frame(Instant::now()) {
        report_now(&mut registry, "every 5 s");
    }
}

/// The next encoder's number since the last present, as `passes.txt` numbers it.
pub fn next_pass() -> u32 {
    PASS.fetch_add(1, Relaxed)
}

crate::seam_export! {
    /// The C++ load-time object's constructor: `on` is its `g_stats_on`.
    ///
    /// When on, the dump folder is made first (the C++'s `DXILCaptureMode` call) and the clock
    /// starts.
    fn dxmt_stats_init(on: bool) {
        ON.store(on, Relaxed);
        if on {
            let _ = crate::dump::on();
            registry().start(Instant::now(), platform::counter());
        }
    }
}

crate::seam_export! {
    /// The C++ load-time object's destructor: the exit report.
    fn dxmt_stats_exit() {
        if on() {
            report_now(&mut registry(), "exit");
        }
    }
}

crate::seam_export! {
    /// `StatId`; ids past the last share it.
    ///
    /// # Safety
    ///
    /// `name` is null (an empty name) or a NUL-terminated string.
    unsafe fn dxmt_stat_id(name: *const c_char) -> u32 {
        // SAFETY: the caller's contract.
        id(&unsafe { c_str(name) }.unwrap_or_default()).0
    }
}

crate::seam_export! {
    /// `StatCount`.
    fn dxmt_stat_count(id: u32, n: u64) {
        count(StatId::from_raw(id), n);
    }
}

crate::seam_export! {
    /// `StatBegin`.
    fn dxmt_stat_begin() -> u64 {
        begin()
    }
}

crate::seam_export! {
    /// `StatEnd`.
    fn dxmt_stat_end(id: u32, t0: u64) {
        end(StatId::from_raw(id), t0);
    }
}

crate::seam_export! {
    /// `StatsFrame`.
    fn dxmt_stats_frame() {
        frame();
    }
}

crate::seam_export! {
    /// `StatsNextPass`.
    fn dxmt_stats_next_pass() -> u32 {
        next_pass()
    }
}

impl StatId {
    /// The id of slot `index`: past the last slot, the last.
    fn clamped(index: usize) -> Self {
        Self(u32::try_from(index.min(MAX_STATS - 1)).unwrap_or(0))
    }

    /// The id the C++ passes back, clamped as [`Self::clamped`].
    fn from_raw(id: u32) -> Self {
        Self::clamped(usize::try_from(id).unwrap_or(MAX_STATS))
    }
}

/// One name's figures on one thread, written only by that thread.
struct Slot {
    calls: AtomicU64,
    ticks: AtomicU64,
    max: AtomicU64,
}

impl Slot {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            ticks: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }
}

/// A thread's counters: written by their thread, read (and `max` reset) by the reporter.
///
/// Relaxed loads and stores without read-modify-write: the C++'s racy reads, at its cost, with no
/// data race.
struct ThreadStats {
    tid: u32,
    depth: AtomicU32,
    outer: AtomicU64,
    slots: Box<[Slot]>,
}

impl ThreadStats {
    fn new(tid: u32) -> Self {
        Self {
            tid,
            depth: AtomicU32::new(0),
            outer: AtomicU64::new(0),
            slots: (0..MAX_STATS).map(|_| Slot::new()).collect(),
        }
    }

    fn slot(&self, id: StatId) -> Option<&Slot> {
        self.slots.get(usize::try_from(id.0).ok()?)
    }

    fn count(&self, id: StatId, n: u64) {
        if let Some(slot) = self.slot(id) {
            add(&slot.calls, n);
        }
    }

    fn enter(&self) {
        self.depth
            .store(self.depth.load(Relaxed).wrapping_add(1), Relaxed);
    }

    /// Ends a timed call of `ticks`; an outermost one adds to the thread's time in D3D12.
    fn record(&self, id: StatId, ticks: u64) {
        if let Some(slot) = self.slot(id) {
            add(&slot.calls, 1);
            add(&slot.ticks, ticks);
            if ticks > slot.max.load(Relaxed) {
                slot.max.store(ticks, Relaxed);
            }
        }
        let depth = self.depth.load(Relaxed).wrapping_sub(1);
        self.depth.store(depth, Relaxed);
        if depth == 0 {
            add(&self.outer, ticks);
        }
    }
}

/// A registered thread and its figures at the last report.
struct Seen {
    stats: &'static ThreadStats,
    outer: u64,
    /// Calls and ticks per slot at the last report.
    slots: Box<[(u64, u64)]>,
}

/// What `stats.txt` is made from.
struct Registry {
    names: Vec<String>,
    threads: Vec<Seen>,
    frames: u64,
    reported_frames: u64,
    last: Instant,
    last_ticks: u64,
    text: Vec<u8>,
}

impl Registry {
    fn new() -> Self {
        Self {
            names: Vec::new(),
            threads: Vec::new(),
            frames: 0,
            reported_frames: 0,
            last: Instant::now(),
            last_ticks: 0,
            text: Vec::new(),
        }
    }

    fn id(&mut self, name: &str) -> StatId {
        self.names.push(name.to_owned());
        StatId::clamped(self.names.len() - 1)
    }

    fn add(&mut self, stats: &'static ThreadStats) {
        self.threads.push(Seen {
            stats,
            outer: 0,
            slots: vec![(0, 0); MAX_STATS].into_boxed_slice(),
        });
    }

    /// Starts the first window at `now`, counter value `ticks`.
    const fn start(&mut self, now: Instant, ticks: u64) {
        self.last = now;
        self.last_ticks = ticks;
    }

    /// Counts a present at `now`: whether a report is due.
    fn frame(&mut self, now: Instant) -> bool {
        self.frames = self.frames.wrapping_add(1);
        now.duration_since(self.last) >= PERIOD
    }

    /// Each thread's figures since the last report, which start again from here.
    ///
    /// `ticks`: the counter's ticks since the last report.
    fn snapshot(&mut self, ticks: u64, name_of: impl Fn(u32) -> String) -> Snapshot {
        let mut snapshot = Snapshot {
            ticks,
            threads: Vec::new(),
            counters: BTreeMap::new(),
        };
        for seen in &mut self.threads {
            let stats = seen.stats;
            let outer = stats.outer.load(Relaxed);
            let mut timed = Vec::new();
            let figures = self.names.iter().zip(&stats.slots).zip(&mut seen.slots);
            for (index, ((name, slot), last)) in figures.enumerate() {
                let (calls, slot_ticks) = (slot.calls.load(Relaxed), slot.ticks.load(Relaxed));
                let max = slot.max.load(Relaxed);
                slot.max.store(0, Relaxed);
                let delta = (calls.wrapping_sub(last.0), slot_ticks.wrapping_sub(last.1));
                *last = (calls, slot_ticks);
                if delta.0 == 0 {
                    continue;
                }
                if let Some(counter) = name.strip_prefix('#') {
                    let total = snapshot.counters.entry(counter.to_owned()).or_default();
                    *total = total.wrapping_add(delta.0);
                } else {
                    timed.push((delta.1, index, delta.0, max));
                }
            }
            let outer_delta = outer.wrapping_sub(seen.outer);
            seen.outer = outer;
            if timed.is_empty() {
                continue;
            }
            timed.sort_unstable_by_key(|&(ticks, index, ..)| Reverse((ticks, index)));
            let lines = timed.into_iter().take(TOP);
            snapshot.threads.push(Block {
                name: name_of(stats.tid),
                tid: stats.tid,
                outer: outer_delta,
                timed: lines
                    .filter_map(|(ticks, index, calls, max)| {
                        Some(Timed {
                            name: self.names.get(index)?.clone(),
                            calls,
                            ticks,
                            max,
                        })
                    })
                    .collect(),
            });
        }
        snapshot
    }

    /// Ends the window at `now`, appends its report to the text and returns the whole text.
    ///
    /// `ticks` is the counter's value then, `hms` the local time.
    fn take_report(
        &mut self,
        why: &str,
        now: Instant,
        ticks: u64,
        hms: [u16; 3],
        name_of: impl Fn(u32) -> String,
    ) -> &[u8] {
        let seconds = now.duration_since(self.last).as_secs_f64();
        let frames = self.frames.wrapping_sub(self.reported_frames);
        let snapshot = self.snapshot(ticks.wrapping_sub(self.last_ticks), name_of);
        let text = report(&snapshot, seconds, frames, why, hms);
        self.text.extend_from_slice(&text);
        self.reported_frames = self.frames;
        self.last_ticks = ticks;
        self.last = now;
        &self.text
    }
}

/// A report's figures, in its order.
struct Snapshot {
    /// The counter's ticks over the window.
    ticks: u64,
    /// The threads with timed calls, in the order they first called.
    threads: Vec<Block>,
    /// The counters by name (no `#`), in byte order.
    counters: BTreeMap<String, u64>,
}

/// A thread's block: its time in outermost calls and its longest calls, longest first.
struct Block {
    name: String,
    tid: u32,
    outer: u64,
    timed: Vec<Timed>,
}

/// A timed call's line.
struct Timed {
    name: String,
    calls: u64,
    ticks: u64,
    max: u64,
}

/// The report of a window of `seconds` and `frames`, as the C++'s `snprintf` lines print it.
fn report(snapshot: &Snapshot, seconds: f64, frames: u64, why: &str, hms: [u16; 3]) -> Vec<u8> {
    let [hour, minute, second] = hms;
    let per_us = if seconds > 0.0 {
        u64_to_f64(snapshot.ticks) / (seconds * 1e6)
    } else {
        1.0
    };
    let per_frame = u64_to_f64(frames);
    let fps = if seconds > 0.0 {
        per_frame / seconds
    } else {
        0.0
    };
    let mut out = Vec::new();
    put(
        &mut out,
        &format!(
            "# {hour:02}:{minute:02}:{second:02} {seconds:.1} s, {frames} frames ({fps:.1} fps), \
             {why}\n"
        ),
    );
    for block in &snapshot.threads {
        let outer = u64_to_f64(block.outer) / per_us;
        put(
            &mut out,
            &format!(
                "thread {} ({}): {outer:.0} us in D3D12",
                block.name, block.tid
            ),
        );
        if frames != 0 {
            put(&mut out, &format!(", {:.1} us/frame", outer / per_frame));
        }
        out.push(b'\n');
        for line in &block.timed {
            let us = u64_to_f64(line.ticks) / per_us;
            let max = u64_to_f64(line.max) / per_us;
            put(
                &mut out,
                &format!(
                    "  {} calls {} us {us:.1} max-us {max:.1}",
                    line.name, line.calls
                ),
            );
            if frames != 0 {
                let calls = u64_to_f64(line.calls) / per_frame;
                put(
                    &mut out,
                    &format!(" | per frame: calls {calls:.1} us {:.1}", us / per_frame),
                );
            }
            out.push(b'\n');
        }
    }
    out.extend_from_slice(b"counters:\n");
    for (name, &n) in &snapshot.counters {
        put(&mut out, &format!("  {name} {n}"));
        if frames != 0 {
            put(
                &mut out,
                &format!(" ({:.1} per frame)", u64_to_f64(n) / per_frame),
            );
        }
        out.push(b'\n');
    }
    let total = |name: &str| snapshot.counters.get(name).copied().unwrap_or(0);
    put(
        &mut out,
        &format!(
            "  encoder boundaries {}, {} with no barrier\n",
            total("encoder boundaries"),
            total("encoder boundaries with no barrier")
        ),
    );
    out
}

/// Appends `piece` as the C++'s `snprintf` into its 512-byte line keeps it.
fn put(out: &mut Vec<u8>, piece: &str) {
    let bytes = piece.as_bytes();
    out.extend_from_slice(bytes.get(..LINE_MAX).unwrap_or(bytes));
}

/// Adds to a counter its own thread writes: a load and a store, no read-modify-write.
fn add(counter: &AtomicU64, n: u64) {
    counter.store(counter.load(Relaxed).wrapping_add(n), Relaxed);
}

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The calling thread's counters, made and registered at its first call.
fn mine() -> &'static ThreadStats {
    MINE.with(|mine| {
        mine.get().unwrap_or_else(|| {
            let stats: &'static ThreadStats =
                Box::leak(Box::new(ThreadStats::new(platform::thread_id())));
            registry().add(stats);
            mine.set(Some(stats));
            stats
        })
    })
}

/// Reports now and saves the whole text as `stats.txt`.
fn report_now(registry: &mut Registry, why: &str) {
    let text = registry.take_report(
        why,
        Instant::now(),
        platform::counter(),
        platform::local_time(),
        platform::thread_name,
    );
    crate::dump::save("stats.txt", text);
}

// The tests read macOS's paths and printf: they run on the host.
#[cfg(all(test, target_os = "macos"))]
mod tests;
