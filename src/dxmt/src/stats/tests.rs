use std::ffi::{CString, c_uint, c_ulong, c_ulonglong};

use super::*;

/// What `snprintf` into the C++'s 512-byte line wrote.
fn printf(call: impl FnOnce(*mut c_char, usize) -> i32) -> Vec<u8> {
    let mut line: [c_char; 512] = [0; 512];
    call(line.as_mut_ptr(), line.len());
    line.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c.cast_unsigned())
        .collect()
}

fn c_text(text: &str) -> CString {
    CString::new(text).unwrap()
}

/// The C++'s `Report` over `snapshot`, its lines built by `snprintf` with its format literals.
fn report_as_printf(
    snapshot: &Snapshot,
    seconds: f64,
    frames: u64,
    why: &str,
    hms: [u16; 3],
) -> Vec<u8> {
    let per_us = if seconds > 0.0 {
        u64_to_f64(snapshot.ticks) / (seconds * 1e6)
    } else {
        1.0
    };
    let f = u64_to_f64(frames);
    let mut out = Vec::new();
    let why = c_text(why);
    let fps = if seconds > 0.0 { f / seconds } else { 0.0 };
    let header = c"# %02u:%02u:%02u %.1f s, %llu frames (%.1f fps), %s\n";
    // SAFETY: the format's arguments, in order, with C's types.
    out.extend(printf(|line, len| unsafe {
        libc::snprintf(
            line,
            len,
            header.as_ptr(),
            c_uint::from(hms[0]),
            c_uint::from(hms[1]),
            c_uint::from(hms[2]),
            seconds,
            c_ulonglong::from(frames),
            fps,
            why.as_ptr(),
        )
    }));
    for block in &snapshot.threads {
        let name = c_text(&block.name);
        let outer = u64_to_f64(block.outer);
        let thread = c"thread %s (%lu): %.0f us in D3D12";
        // SAFETY: the format's arguments, in order, with C's types.
        out.extend(printf(|line, len| unsafe {
            libc::snprintf(
                line,
                len,
                thread.as_ptr(),
                name.as_ptr(),
                c_ulong::from(block.tid),
                outer / per_us,
            )
        }));
        if frames != 0 {
            // SAFETY: the format's arguments, in order, with C's types.
            out.extend(printf(|line, len| unsafe {
                libc::snprintf(line, len, c", %.1f us/frame".as_ptr(), outer / per_us / f)
            }));
        }
        out.push(b'\n');
        for timed in &block.timed {
            let name = c_text(&timed.name);
            let ticks = u64_to_f64(timed.ticks);
            let call = c"  %s calls %llu us %.1f max-us %.1f";
            // SAFETY: the format's arguments, in order, with C's types.
            out.extend(printf(|line, len| unsafe {
                libc::snprintf(
                    line,
                    len,
                    call.as_ptr(),
                    name.as_ptr(),
                    c_ulonglong::from(timed.calls),
                    ticks / per_us,
                    u64_to_f64(timed.max) / per_us,
                )
            }));
            if frames != 0 {
                let per_frame = c" | per frame: calls %.1f us %.1f";
                // SAFETY: the format's arguments, in order, with C's types.
                out.extend(printf(|line, len| unsafe {
                    libc::snprintf(
                        line,
                        len,
                        per_frame.as_ptr(),
                        u64_to_f64(timed.calls) / f,
                        ticks / per_us / f,
                    )
                }));
            }
            out.push(b'\n');
        }
    }
    out.extend_from_slice(b"counters:\n");
    for (name, &n) in &snapshot.counters {
        let name = c_text(name);
        // SAFETY: the format's arguments, in order, with C's types.
        out.extend(printf(|line, len| unsafe {
            libc::snprintf(
                line,
                len,
                c"  %s %llu".as_ptr(),
                name.as_ptr(),
                c_ulonglong::from(n),
            )
        }));
        if frames != 0 {
            // SAFETY: the format's arguments, in order, with C's types.
            out.extend(printf(|line, len| unsafe {
                libc::snprintf(line, len, c" (%.1f per frame)".as_ptr(), u64_to_f64(n) / f)
            }));
        }
        out.push(b'\n');
    }
    let total = |name: &str| snapshot.counters.get(name).copied().unwrap_or(0);
    let boundaries = c"  encoder boundaries %llu, %llu with no barrier\n";
    // SAFETY: the format's arguments, in order, with C's types.
    out.extend(printf(|line, len| unsafe {
        libc::snprintf(
            line,
            len,
            boundaries.as_ptr(),
            c_ulonglong::from(total("encoder boundaries")),
            c_ulonglong::from(total("encoder boundaries with no barrier")),
        )
    }));
    out
}

fn timed(name: &str, calls: u64, ticks: u64, max: u64) -> Timed {
    Timed {
        name: name.to_owned(),
        calls,
        ticks,
        max,
    }
}

/// A thread's counters as a test registers them: leaked, as the live ones are.
fn thread(registry: &mut Registry, tid: u32) -> &'static ThreadStats {
    let stats: &'static ThreadStats = Box::leak(Box::new(ThreadStats::new(tid)));
    registry.add(stats);
    stats
}

fn names(tid: u32) -> String {
    format!("worker-{tid}")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn header_matches_perf_py() {
    let snapshot = Snapshot {
        ticks: 0,
        threads: Vec::new(),
        counters: BTreeMap::new(),
    };
    let out = report(&snapshot, 5.0004, 485, "every 5 s", [9, 5, 7]);
    let header = text(&out).lines().next().unwrap();
    assert_eq!(header, "# 09:05:07 5.0 s, 485 frames (97.0 fps), every 5 s");
    // perf.py's fps(): ^# [\d:]+ ([\d.]+) s, (\d+) frames \([\d.]+ fps\), every [\d.]+ s$
    let digits =
        |s: &str, extra: char| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == extra);
    let rest = header.strip_prefix("# ").unwrap();
    let (time, rest) = rest.split_once(' ').unwrap();
    let (seconds, rest) = rest.split_once(" s, ").unwrap();
    let (frames, rest) = rest.split_once(" frames (").unwrap();
    let (fps, rest) = rest.split_once(" fps), every ").unwrap();
    let period = rest.strip_suffix(" s").unwrap();
    assert!(digits(time, ':') && digits(seconds, '.') && digits(frames, '0'));
    assert!(digits(fps, '.') && digits(period, '.'));
    assert_eq!(
        (
            seconds.parse::<f64>().unwrap(),
            frames.parse::<u64>().unwrap()
        ),
        (5.0, 485)
    );
    assert_eq!(
        out,
        report_as_printf(&snapshot, 5.0004, 485, "every 5 s", [9, 5, 7])
    );
}

#[test]
fn thread_and_call_lines_as_printf() {
    // Windows of no time and of 0.05 to 7.3 s, counter ticks for rates up to 20 a microsecond
    // (a counter that does not move over a window that has a length is not on the grid: no
    // counter is that slow, and 0/0 is NaN, which printf and Rust spell apart), and counts whose
    // quotients fall on %.1f's and %.0f's rounding ties (0.25, 0.75, 2.5, 0.05).
    let long = "n".repeat(600);
    for seconds in [0.0, 0.05, 1.0, 5.25, 7.3] {
        for ticks in [
            1,
            50_000,
            1_000_000,
            2_000_000,
            3_300_000,
            4_000_000,
            146_000_000,
        ] {
            for frames in [0, 1, 4, 8, 20, 485] {
                let snapshot = Snapshot {
                    ticks,
                    threads: vec![
                        Block {
                            name: "dxmt-encode-thread".to_owned(),
                            tid: 4_294_967_295,
                            outer: 5,
                            timed: vec![
                                timed("list.Close", 1, 1, 1),
                                timed("queue.ExecuteCommandLists", 3, 10, 6),
                                timed("device.CreateGraphicsPipelineState", 2, 25, 25),
                                timed("x", 1, 0, 0),
                            ],
                        },
                        Block {
                            name: long.clone(),
                            tid: 0,
                            outer: 1_234_567_890_123,
                            timed: vec![timed(&long, u64::MAX, u64::MAX / 3, 7)],
                        },
                    ],
                    counters: BTreeMap::from([
                        ("encoder boundaries".to_owned(), 5),
                        ("render passes".to_owned(), 1),
                        (long.clone(), 3),
                    ]),
                };
                let ours = report(&snapshot, seconds, frames, "exit", [23, 59, 0]);
                let cpp = report_as_printf(&snapshot, seconds, frames, "exit", [23, 59, 0]);
                assert_eq!(
                    text(&ours),
                    text(&cpp),
                    "{seconds} s, {ticks} ticks, {frames} frames"
                );
            }
        }
    }
}

#[test]
fn counters_sum_by_name_in_order() {
    let mut registry = Registry::new();
    let ids: Vec<StatId> = ["#b", "#a", "draw", "#b", "#a z", "#"]
        .into_iter()
        .map(|name| registry.id(name))
        .collect();
    let (first, second) = (thread(&mut registry, 1), thread(&mut registry, 2));
    first.count(ids[0], 3);
    first.count(ids[1], 2);
    first.count(ids[3], 4);
    second.count(ids[1], 5);
    second.count(ids[4], 1);
    second.count(ids[5], 9);
    let start = Instant::now();
    registry.start(start, 0);
    let out = registry.take_report("exit", start + PERIOD, 1, [0; 3], names);
    assert_eq!(
        text(out),
        "# 00:00:00 5.0 s, 0 frames (0.0 fps), exit\ncounters:\n   9\n  a 7\n  a z 1\n  b 7\n  \
         encoder boundaries 0, 0 with no barrier\n"
    );
}

#[test]
fn boundaries_line_always_last() {
    let snapshot = |counters: &[(&str, u64)]| Snapshot {
        ticks: 10,
        threads: Vec::new(),
        counters: counters.iter().map(|&(n, c)| (n.to_owned(), c)).collect(),
    };
    let none = report(&snapshot(&[]), 1.0, 2, "exit", [0; 3]);
    assert!(text(&none).ends_with("counters:\n  encoder boundaries 0, 0 with no barrier\n"));
    let some = snapshot(&[
        ("encoder boundaries", 5),
        ("encoder boundaries with no barrier", 2),
        ("zz", 1),
    ]);
    let out = report(&some, 1.0, 2, "exit", [0; 3]);
    assert!(
        text(&out).ends_with("  zz 1 (0.5 per frame)\n  encoder boundaries 5, 2 with no barrier\n")
    );
    assert!(text(&out).contains("\n  encoder boundaries 5 (2.5 per frame)\n"));
    assert_eq!(out, report_as_printf(&some, 1.0, 2, "exit", [0; 3]));
}

#[test]
fn ids_past_the_last_share_it() {
    let mut registry = Registry::new();
    let ids: Vec<StatId> = (0..MAX_STATS + 6)
        .map(|n| registry.id(&format!("call{n}")))
        .collect();
    assert_eq!(ids[MAX_STATS - 2].0, 1022);
    assert!(ids[MAX_STATS - 1..].iter().all(|id| id.0 == 1023));
    let stats = thread(&mut registry, 7);
    stats.enter();
    stats.record(ids[MAX_STATS + 5], 4);
    stats.enter();
    stats.record(ids[MAX_STATS - 1], 2);
    stats.enter();
    stats.record(StatId::from_raw(u32::MAX), 1);
    let snapshot = registry.snapshot(1, names);
    let lines = &snapshot.threads[0].timed;
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].name.as_str(), lines[0].calls, lines[0].ticks),
        ("call1023", 3, 7)
    );
}

#[test]
fn max_resets_per_report() {
    let mut registry = Registry::new();
    let id = registry.id("list.Reset");
    let stats = thread(&mut registry, 3);
    for ticks in [100, 50] {
        stats.enter();
        stats.record(id, ticks);
    }
    let first = registry.snapshot(1, names);
    assert_eq!(
        (
            first.threads[0].timed[0].max,
            first.threads[0].timed[0].calls
        ),
        (100, 2)
    );
    stats.enter();
    stats.record(id, 30);
    let second = registry.snapshot(1, names);
    let line = &second.threads[0].timed[0];
    assert_eq!((line.max, line.calls, line.ticks), (30, 1, 30));
    assert!(registry.snapshot(1, names).threads.is_empty());
}

#[test]
fn timed_lines_sorted_desc_ties_by_id_top_30() {
    let mut registry = Registry::new();
    let ids: Vec<StatId> = (0..40).map(|n| registry.id(&format!("c{n:02}"))).collect();
    let stats = thread(&mut registry, 9);
    for (n, &id) in ids.iter().enumerate() {
        stats.enter();
        stats.record(id, u64::try_from(n % 7).unwrap() + 1);
    }
    let snapshot = registry.snapshot(1, names);
    let order: Vec<&str> = snapshot.threads[0]
        .timed
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    let mut expected: Vec<(usize, usize)> = (0..40).map(|n| (n % 7 + 1, n)).collect();
    expected.sort_by(|a, b| b.cmp(a));
    let expected: Vec<String> = expected
        .iter()
        .take(TOP)
        .map(|&(_, n)| format!("c{n:02}"))
        .collect();
    assert_eq!(order, expected);
    assert_eq!(&order[..4], ["c34", "c27", "c20", "c13"]);
}

#[test]
fn scope_of_10ms_reports_10000us() {
    // A 24 MHz counter over a 1 s window: a scope of 240000 ticks is 10 ms.
    let mut registry = Registry::new();
    let outer = registry.id("queue.Present");
    let inner = registry.id("queue.Signal");
    let stats = thread(&mut registry, 12);
    stats.enter();
    stats.enter();
    stats.record(inner, 24_000);
    stats.record(outer, 240_000);
    let start = Instant::now();
    registry.start(start, 1_000);
    let out = registry.take_report(
        "exit",
        start + Duration::from_secs(1),
        24_001_000,
        [1, 2, 3],
        names,
    );
    assert_eq!(
        text(out),
        "# 01:02:03 1.0 s, 0 frames (0.0 fps), exit\nthread worker-12 (12): 10000 us in D3D12\n  \
         queue.Present calls 1 us 10000.0 max-us 10000.0\n  queue.Signal calls 1 us 1000.0 max-us \
         1000.0\ncounters:\n  encoder boundaries 0, 0 with no barrier\n"
    );
}

#[test]
fn frame_reports_after_5s() {
    let mut registry = Registry::new();
    let start = Instant::now();
    registry.start(start, 0);
    assert!(!registry.frame(start + Duration::from_millis(4_999)));
    assert!(registry.frame(start + PERIOD));
    let first = registry
        .take_report("every 5 s", start + PERIOD, 0, [0; 3], names)
        .to_vec();
    assert!(text(&first).starts_with("# 00:00:00 5.0 s, 2 frames (0.4 fps), every 5 s\n"));
    assert!(!registry.frame(start + Duration::from_millis(9_999)));
    assert!(registry.frame(start + Duration::from_secs(10)));
    let both = registry.take_report("exit", start + Duration::from_secs(11), 0, [0; 3], names);
    let second = both.strip_prefix(first.as_slice()).unwrap();
    assert!(text(second).starts_with("# 00:00:00 6.0 s, 2 frames (0.3 fps), exit\n"));
}

#[test]
fn next_pass_restarts_each_frame() {
    ON.store(true, Relaxed);
    frame();
    assert_eq!([next_pass(), next_pass(), next_pass()], [0, 1, 2]);
    frame();
    assert_eq!(next_pass(), 0);
    ON.store(false, Relaxed);
    frame();
    assert_eq!(next_pass(), 1);
}

#[test]
fn stat_id_copies_the_name() {
    let name = c_text("#copied");
    // SAFETY: a NUL-terminated string.
    let id = unsafe { dxmt_stat_id(name.as_ptr()) };
    drop(name);
    assert_eq!(registry().names[usize::try_from(id).unwrap()], "#copied");
    // SAFETY: null is an empty name.
    let empty = unsafe { dxmt_stat_id(core::ptr::null()) };
    assert_eq!(registry().names[usize::try_from(empty).unwrap()], "");
}
