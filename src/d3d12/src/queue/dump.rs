//! Pass dumps, F9 frame dumps, pixel history, and Metal frame capture
//!
//! (`d3d12_command_queue.cpp:730-1226, 2128-2157`).

use winemetal_sys::Obj;

use crate::encoder::EncoderData;

/// Process-wide pass-dump and pixel-history state (`d3d12_command_queue.cpp:760-823`).
pub struct Dumps {
    pub active: bool,
    pub frame: u32,
    pub pass: u32,
}

impl Dumps {
    /// Creates an inactive dump state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: false,
            frame: 0,
            pass: 0,
        }
    }
}

impl Default for Dumps {
    fn default() -> Self {
        Self::new()
    }
}

/// Advances F9 pass dumps and Metal frame capture at the end of a present
///
/// (`d3d12_command_queue.cpp:2128-2157`).
pub fn on_present(_mtl_device: &Obj) {
    let (): () = crate::not_implemented!("queue::dump::on_present");
}

/// Formats the `passes.txt` label for pass `index` (`PassLabel`,
///
/// `d3d12_command_queue.cpp:855`).
#[must_use]
pub fn pass_label(_index: usize, _encoder: &EncoderData) -> String {
    let (): () = crate::not_implemented!("queue::dump::pass_label");
    String::new()
}
