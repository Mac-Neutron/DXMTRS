//! The core's log, written through the front end's `dxmt_log`.
//!
//! The core holds no log file: each front end defines `dxmt_log(level, message, length)` over its
//! own `dxmt_util` logger, so one writer owns each log file. The level counts up from trace (0)
//! through debug, info and warning to error (4); the message is UTF-8 without a terminator.
//! With `perf-build`, info, debug and trace write nothing.

#[cfg(test)]
use std::cell::RefCell;

/// A message's level: the number `dxmt_log` takes.
#[repr(u32)]
enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(not(test))]
unsafe extern "C" {
    /// The front end's writer.
    fn dxmt_log(level: u32, message: *const u8, length: usize);
}

#[cfg(test)]
thread_local! {
    /// What this thread's test logged: the level and the text of each message.
    static CAPTURED: RefCell<Vec<(u32, String)>> = const { RefCell::new(Vec::new()) };
}

/// Logs an error.
pub fn err(message: &str) {
    write(Level::Error, message);
}

/// Logs a warning.
pub fn warn(message: &str) {
    write(Level::Warn, message);
}

/// Logs at info level (nothing in the performance build).
pub fn info(message: &str) {
    below_warn(Level::Info, message);
}

/// Logs at debug level (nothing in the performance build).
pub fn debug(message: &str) {
    below_warn(Level::Debug, message);
}

/// Logs at trace level (nothing in the performance build).
pub fn trace(message: &str) {
    below_warn(Level::Trace, message);
}

/// What this thread logged since the last call, as the level and the text of each message.
#[cfg(test)]
#[must_use]
pub fn take_captured() -> Vec<(u32, String)> {
    CAPTURED.take()
}

fn below_warn(level: Level, message: &str) {
    if cfg!(not(feature = "perf-build")) {
        write(level, message);
    }
}

#[cfg(not(test))]
fn write(level: Level, message: &str) {
    // SAFETY: the pointer and length describe `message`, alive for the call; the front end's
    // `dxmt_log` only reads them.
    unsafe { dxmt_log(level as u32, message.as_ptr(), message.len()) };
}

#[cfg(test)]
fn write(level: Level, message: &str) {
    CAPTURED.with_borrow_mut(|log| log.push((level as u32, message.to_owned())));
}

#[cfg(test)]
mod tests;
