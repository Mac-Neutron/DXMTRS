//! Labels on Metal's developer HUD: the version line and, in a debug build, a line per frame.

use std::ffi::CString;

use winemetal_sys::{
    DeveloperHUDProperties_addLabel, DeveloperHUDProperties_instance,
    DeveloperHUDProperties_remove, DeveloperHUDProperties_updateLabel, NSString_alloc_init,
    NSString_string, Obj, Pool, wire::WMTUTF8_STRING_ENCODING,
};

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
