//! The Metal frame capture a developer starts with a key or a frame number.

use std::ffi::CString;

use dxmt_util::env;
use winemetal_sys::{
    MTLCaptureManager_sharedCaptureManager, MTLCaptureManager_startCapture,
    MTLCaptureManager_stopCapture, Obj,
    wire::{MemPtr, WMT_CAPTURE_DESTINATION_GPU_TRACE_DOCUMENT, WMTCaptureInfo},
};

use crate::{log, settings::settings};

/// Starts a Metal GPU trace capture for `device` at `frame`.
pub fn start_capture(device: &Obj, frame: u64) {
    let exe = env::exe_base_name();
    let path = format!("{exe}_F.{frame}.gputrace");
    log::warn(&format!("A new capture will be saved to {path}"));
    let Ok(cpath) = CString::new(path) else {
        return;
    };
    let output_url = MemPtr::from_ptr(cpath.as_ptr());
    let mut info = WMTCaptureInfo {
        capture_object: device.handle(),
        output_url,
        destination: WMT_CAPTURE_DESTINATION_GPU_TRACE_DOCUMENT,
    };
    // SAFETY: `sharedCaptureManager` returns the process singleton.
    let mgr = unsafe { MTLCaptureManager_sharedCaptureManager() };
    // SAFETY: `mgr` is the singleton; `info` and `cpath` outlive `startCapture`.
    let _ = unsafe { MTLCaptureManager_startCapture(mgr, &raw mut info) };
}

/// Stops an active Metal GPU trace capture.
pub fn stop_capture() {
    // SAFETY: `sharedCaptureManager` returns the process singleton.
    let mgr = unsafe { MTLCaptureManager_sharedCaptureManager() };
    // SAFETY: `mgr` is the singleton.
    unsafe { MTLCaptureManager_stopCapture(mgr) };
}

/// What the encode thread does about a capture at a frame.
#[derive(PartialEq, Eq, Debug)]
pub enum NextAction {
    StartCapture,
    StopCapture,
    Nothing,
}

/// Where a capture is: not enabled, waiting, scheduled or running.
enum State {
    Freezed,
    Idle,
    WillCapture,
    Capturing,
}

/// The capture's state, which the encode thread and the present path drive.
///
/// Behaviour is the C++ `CaptureState`'s. Written ahead for the switches of `d3d11.dll` and
/// `d3d12.dll`, which are its first callers.
pub struct CaptureState {
    state: State,
    next_capture_frame: u64,
    key_pressed: bool,
}

impl CaptureState {
    /// A capture that runs when `enabled`, and says so in the log.
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        if enabled {
            log::warn("DXMT capture enabled");
        }
        Self {
            state: if enabled { State::Idle } else { State::Freezed },
            next_capture_frame: 0,
            key_pressed: false,
        }
    }

    /// A capture enabled as the environment says.
    ///
    /// `MTL_CAPTURE_ENABLED` is `1` and this program is the one `DXMT_CAPTURE_EXECUTABLE` names.
    #[must_use]
    pub fn from_env() -> Self {
        let settings = settings();
        Self::new(enabled_by(
            settings.mtl_capture_enabled.as_deref(),
            &env::exe_base_name(),
            settings.capture_executable.as_deref(),
        ))
    }

    /// The action `frame` calls for: a scheduled capture starts at its frame and stops after it.
    pub const fn next_action(&mut self, frame: u64) -> NextAction {
        match self.state {
            State::WillCapture if frame == self.next_capture_frame => {
                self.state = State::Capturing;
                NextAction::StartCapture
            }
            State::Capturing if frame > self.next_capture_frame => {
                self.next_capture_frame = 0;
                self.state = State::Idle;
                NextAction::StopCapture
            }
            _ => NextAction::Nothing,
        }
    }

    /// Schedules a capture of frame `next`; nothing happens unless the capture is idle.
    pub const fn schedule(&mut self, next: u64) {
        if matches!(self.state, State::Idle) {
            self.state = State::WillCapture;
            self.next_capture_frame = next;
        }
    }

    /// Whether F10 was just pressed while the capture is idle: true once per press.
    ///
    /// `f10_down` is the key's state, which the front end reads (`GetAsyncKeyState(VK_F10)`,
    /// bit 0x8000; natively there is no key and it passes false). While the capture is not idle
    /// the key is not looked at.
    pub const fn should_capture_next(&mut self, f10_down: bool) -> bool {
        if !matches!(self.state, State::Idle) {
            return false;
        }
        if f10_down && !self.key_pressed {
            self.key_pressed = true;
            return true;
        }
        self.key_pressed = f10_down;
        false
    }
}

/// The capture the environment asks for, with the frame `DXMT_CAPTURE_FRAME` names scheduled.
///
/// [`CaptureState::from_env`], scheduled as the core's encode thread reads the variable.
/// The frame is the variable's whole value, spaces around it aside, and above 0: anything else
/// schedules nothing.
#[must_use]
pub fn schedule_from_env() -> CaptureState {
    let mut capture = CaptureState::from_env();
    if let Some(frame) = crate::settings::settings().capture_frame {
        capture.schedule(frame);
    }
    capture
}

/// The frame `DXMT_CAPTURE_FRAME`'s `value` names: a whole number above 0, trimmed, or none.
#[must_use]
pub fn capture_frame(value: Option<&str>) -> Option<u64> {
    value?.trim().parse().ok().filter(|frame| *frame > 0)
}

/// Whether the capture is enabled: `mtl_capture_enabled` is `1` and `exe` is `target`.
///
/// An unset variable reads as the empty string, as the C++'s `getEnvVar` returns it, so a program
/// without a name (the native dylibs) is the one an unset `DXMT_CAPTURE_EXECUTABLE` names.
fn enabled_by(mtl_capture_enabled: Option<&str>, exe: &str, target: Option<&str>) -> bool {
    mtl_capture_enabled.unwrap_or_default() == "1" && exe == target.unwrap_or_default()
}

#[cfg(test)]
mod tests;
