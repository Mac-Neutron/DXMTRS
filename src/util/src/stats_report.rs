//! `stats.txt`'s report: a window's threads, timed calls and counters, as DXMT's C++ prints it.
//!
//! D3D12's `DXMT_STATS` (`dxmt-core`'s `stats`) and D3D9's perf-tracking builds write it; each
//! fills a [`Snapshot`] and appends what [`report`] renders.

use std::collections::BTreeMap;

use crate::float_cast::u64_to_f64;

/// How much of a piece the 512-byte line buffer keeps.
const LINE_MAX: usize = 511;

/// A report's figures, in its order.
pub struct Snapshot {
    /// The counter's ticks over the window.
    pub ticks: u64,
    /// The threads with timed calls, in the order they first called.
    pub threads: Vec<Block>,
    /// The counters by name (no `#`), in byte order.
    pub counters: BTreeMap<String, u64>,
}

/// A thread's block: its time in outermost calls and its longest calls, longest first.
pub struct Block {
    pub name: String,
    pub tid: u32,
    pub outer: u64,
    pub timed: Vec<Timed>,
}

/// A timed call's line.
pub struct Timed {
    pub name: String,
    pub calls: u64,
    pub ticks: u64,
    pub max: u64,
}

/// The report of a window of `seconds` and `frames`, as the C++'s `snprintf` lines print it.
#[must_use]
pub fn report(snapshot: &Snapshot, seconds: f64, frames: u64, why: &str, hms: [u16; 3]) -> Vec<u8> {
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

#[cfg(test)]
mod tests;
