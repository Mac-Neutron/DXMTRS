//! Labels on Metal's developer HUD: the version line and, in a debug build, a line per frame.

use std::ffi::CString;

use winemetal_sys::{
    DeveloperHUDProperties_addLabel, DeveloperHUDProperties_instance,
    DeveloperHUDProperties_remove, DeveloperHUDProperties_updateLabel, NSString_alloc_init,
    NSString_string, Obj, Pool, wire::WMTUTF8_STRING_ENCODING,
};

use crate::frame_stats::{CompatibilityFlags, Container, ScalerType};

/// The HUD's graph label, which the version label goes after.
const DEFAULT_GRAPH: &str = "com.apple.hud-graph.default";

/// The labels this program put on Metal's developer HUD.
///
/// Behaviour is the C++ `HUDState`'s; the lines are written only when `debug_assertions` is on,
/// which is the build the C++'s `DXMT_DEBUG` names. A temporary string lives in a pool of its own
/// call, not in one from `begin` to `end`.
pub struct Hud {
    /// The HUD, a singleton Metal keeps: borrowed.
    properties: u64,
    /// The version label, then a label per line.
    labels: Vec<Obj>,
    /// The line the next [`Self::print_line`] writes.
    current_line: usize,
}

impl Hud {
    /// The process's developer HUD.
    #[must_use]
    pub fn instance() -> Self {
        // SAFETY: no precondition; the HUD is a singleton that is never released.
        let properties = unsafe { DeveloperHUDProperties_instance() };
        Self {
            properties,
            labels: Vec::new(),
            current_line: 0,
        }
    }

    /// Adds the version label with `heading` as its text.
    pub fn initialize(&mut self, heading: &str) {
        let _pool = Pool::new();
        let Some(label) = new_label("com.github.3shain.dxmt-version") else {
            return;
        };
        // SAFETY: the HUD and both labels are live objects; the pool outlives the call and drains
        // the autoreleased graph name.
        unsafe {
            DeveloperHUDProperties_addLabel(
                self.properties,
                label.handle(),
                temp_string(DEFAULT_GRAPH.as_bytes()),
            );
        }
        self.update(&label, heading);
        self.labels.push(label);
    }

    /// Starts a frame's lines at the first below the version.
    pub const fn begin(&mut self) {
        if cfg!(debug_assertions) {
            self.current_line = 1;
        }
    }

    /// Writes the next line, adding a label for it if there is none yet.
    pub fn print_line(&mut self, text: &str) {
        if cfg!(not(debug_assertions)) {
            return;
        }
        let _pool = Pool::new();
        while self.current_line >= self.labels.len() {
            let after = self.labels.last().map_or(0, Obj::handle);
            let name = format!("com.github.3shain.dxmt-line{}", self.labels.len());
            let Some(label) = new_label(&name) else {
                return;
            };
            // SAFETY: the HUD and both labels are live objects (`after` is the last label or the
            // null handle, which Metal puts first).
            unsafe { DeveloperHUDProperties_addLabel(self.properties, label.handle(), after) };
            self.labels.push(label);
        }
        if let Some(label) = self.labels.get(self.current_line) {
            self.update(label, text);
        }
        self.current_line += 1;
    }

    /// Takes off the labels past the lines this frame wrote.
    pub fn end(&mut self) {
        if cfg!(not(debug_assertions)) {
            return;
        }
        while self.labels.len() > self.current_line {
            let Some(label) = self.labels.pop() else {
                break;
            };
            // SAFETY: the HUD and the label are live objects.
            unsafe { DeveloperHUDProperties_remove(self.properties, label.handle()) };
        }
    }

    /// Writes the frame's statistics lines (`UpdateStatistics`), [`statistics_lines`]' text.
    pub fn update_statistics(&mut self, statistics: &Container, frame_id: u64) {
        self.begin();
        statistics_lines(statistics, frame_id, |line| self.print_line(line));
        self.end();
    }

    fn update(&self, label: &Obj, text: &str) {
        // SAFETY: the HUD and the label are live objects; the caller's pool drains the temporary
        // string.
        unsafe {
            DeveloperHUDProperties_updateLabel(
                self.properties,
                label.handle(),
                temp_string(text.as_bytes()),
            );
        };
    }
}

/// The HUD's statistics lines for `frame_id` (`UpdateStatistics`), each passed to `line`.
///
/// The frame before `frame_id`, against the window's average and maximum: compatibility flags,
/// commits, syncs, encode times, render and clear passes, then the `MetalFX` scaler when one ran.
pub fn statistics_lines(statistics: &Container, frame_id: u64, mut line: impl FnMut(&str)) {
    let frame = statistics.at(frame_id.wrapping_sub(1));
    let average = statistics.average();
    let max = statistics.max();
    let flags = frame.compatibility_flags;
    let mut text = *b"---------------------------";
    for (flag, at, mark) in [
        (CompatibilityFlags::UNSUPPORTED_GEOMETRY_DRAW, 3, &b"G"[..]),
        (
            CompatibilityFlags::UNSUPPORTED_TESSELLATION_OUTPUT_PRIMITIVE,
            5,
            b"TO",
        ),
        (
            CompatibilityFlags::UNSUPPORTED_INDIRECT_TESSELLATION_DRAW,
            8,
            b"IT",
        ),
        (
            CompatibilityFlags::UNSUPPORTED_GEOMETRY_TESSELLATION_DRAW,
            11,
            b"GT",
        ),
        (CompatibilityFlags::UNSUPPORTED_DRAW_AUTO, 14, b"A"),
        (CompatibilityFlags::UNSUPPORTED_PREDICATION, 16, b"P"),
        (
            CompatibilityFlags::UNSUPPORTED_STREAM_OUTPUT_APPENDING,
            18,
            b"SA",
        ),
        (
            CompatibilityFlags::UNSUPPORTED_MULTIPLE_STREAM_OUTPUT,
            21,
            b"MS",
        ),
    ] {
        if flags.contains(flag)
            && let Some(slot) = text.get_mut(at..at + mark.len())
        {
            slot.copy_from_slice(mark);
        }
    }
    if let Ok(flags_line) = core::str::from_utf8(&text) {
        line(flags_line);
    }
    let ms = |d: core::time::Duration| (d.as_secs_f64() * 1000.0).min(99.9);
    line(&format!(
        "Commit: {:2} -{:4.1} -{:4.1}",
        frame.command_buffer_count.min(99),
        ms(average.commit_interval),
        ms(max.commit_interval),
    ));
    line(&format!(
        "Sync:   {:2} {:4.1}  {:2} {:4.1} {:2}",
        frame.sync_count.min(99),
        ms(average.sync_interval),
        max.event_stall.min(99),
        ms(average.present_latency_interval),
        frame.latency,
    ));
    let prep = ms(average.encode_prepare_interval);
    let flush_non_block = ms(average
        .encode_flush_interval
        .saturating_sub(average.drawable_blocking_interval));
    let block = ms(average.drawable_blocking_interval);
    let total = ms(average.encode_prepare_interval + average.encode_flush_interval);
    line(&format!(
        "Encode: {prep:4.1}+{flush_non_block:4.1}+{block:4.1}={total:4.1}"
    ));
    line(&format!(
        "Render:{:3}+{:<3} Clear:{:3}+{:<2}",
        frame
            .render_pass_count
            .saturating_sub(frame.render_pass_optimized)
            .min(999),
        frame.render_pass_optimized.min(999),
        frame
            .clear_pass_count
            .saturating_sub(frame.clear_pass_optimized)
            .min(999),
        frame.clear_pass_optimized.min(99),
    ));
    let info = &frame.last_scaler_info;
    match info.kind {
        ScalerType::Temporal => {
            let aexp = if info.auto_exposure { "AEXP" } else { "" };
            let hmv = if info.motion_vector_highres {
                "HMV"
            } else {
                ""
            };
            line(&format!("MetalFX: Temporal {aexp} {hmv}"));
        }
        ScalerType::Spatial => line("MetalFX: Spatial"),
        ScalerType::None => return,
    }
    line(&format!(
        "Scale: {:4}x{:4}->{:4}x{:4}",
        info.input_width, info.input_height, info.output_width, info.output_height
    ));
}

/// A label named `name`, which the caller owns.
fn new_label(name: &str) -> Option<Obj> {
    let name = c_string(name.as_bytes());
    // SAFETY: `name` is NUL-terminated and alive for the call.
    let label = unsafe { NSString_alloc_init(name.as_ptr(), WMTUTF8_STRING_ENCODING) };
    // SAFETY: `alloc` and `init` return a new string with its one reference.
    unsafe { Obj::adopt(label) }
}

/// An autoreleased string of `text`, borrowed for as long as the caller's pool lives.
fn temp_string(text: &[u8]) -> u64 {
    let text = c_string(text);
    // SAFETY: `text` is NUL-terminated and alive for the call.
    unsafe { NSString_string(text.as_ptr(), WMTUTF8_STRING_ENCODING) }
}

/// `text` up to its first NUL, as a C string takes it.
fn c_string(text: &[u8]) -> CString {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    CString::new(text.get(..end).unwrap_or_default()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
