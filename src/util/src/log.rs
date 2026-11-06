//! DXMT's log, written the way `util/log/log.cpp` writes it.
//!
//! The level comes from `DXMT_LOG_LEVEL` (`info` unless it names another). Each line of a message
//! gets the level's prefix and goes to Wine's debug output (stderr outside Wine) and to
//! `<DXMT_LOG_PATH>/<exe>_<dll>.log`: no file when the path is `none`, or unset while Wine's output
//! exists.

use std::{fmt::Write as _, fs::File, io::Write, sync::Mutex};

use crate::env;

/// A message's level, ordered as DXMT's `LogLevel`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    None,
}

/// The level `DXMT_LOG_LEVEL`'s value names.
fn level_from(value: Option<&str>) -> Level {
    match value {
        Some("trace") => Level::Trace,
        Some("debug") => Level::Debug,
        Some("warn") => Level::Warn,
        Some("error") => Level::Error,
        Some("none") => Level::None,
        _ => Level::Info,
    }
}

/// Each line of `message` with the level's prefix, as `std::getline` splits it.
fn lines(level: &Level, message: &str) -> String {
    let prefix = match level {
        Level::Trace => "trace: ",
        Level::Debug => "debug: ",
        Level::Info => "info:  ",
        Level::Warn => "warn:  ",
        Level::Error | Level::None => "err:   ",
    };
    message
        .split_terminator('\n')
        .fold(String::new(), |mut text, line| {
            let _ = writeln!(text, "{prefix}{line}");
            text
        })
}

/// `text` as a text-mode stream writes it.
///
/// On Windows `std::ofstream` and `std::cerr` turn each `\n` into `\r\n`; Wine's debug output
/// takes the `\n` form.
fn text_mode(text: &str, crlf: bool) -> String {
    if crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

/// The log file's path: `<log_path>/<exe's base name>_<file>`.
fn file_name(
    log_path: Option<&str>,
    wine_output: bool,
    exe_path: &str,
    file: &str,
) -> Option<String> {
    let path = log_path.unwrap_or("");
    if path == "none" || (path.is_empty() && wine_output) {
        return None;
    }
    let base = env::base_of(env::name_of(exe_path));
    let slash = if path.is_empty() || path.ends_with('/') {
        ""
    } else {
        "/"
    };
    Some(format!("{path}{slash}{base}_{file}"))
}

/// Wine's `__wine_dbg_output`, which ntdll exports.
type WineOutput = unsafe extern "C" fn(*const core::ffi::c_char) -> i32;

/// The log's state: the level from the first message, the outputs from the first that passes it.
struct State {
    level: Level,
    sinks: Option<Sinks>,
}

/// Where a passing message goes: Wine's debug output or stderr, and the log file.
struct Sinks {
    wine: Option<WineOutput>,
    file: Option<File>,
}

impl Sinks {
    fn open(file: &str, env: &impl Fn(&str) -> Option<String>) -> Self {
        let wine = wine_output();
        let exe = env::exe_path();
        let path = env("DXMT_LOG_PATH");
        let file = file_name(path.as_deref(), wine.is_some(), &exe, file)
            .and_then(|name| File::create(name).ok());
        Self { wine, file }
    }
}

/// The process's environment variable `name`, if set to text.
fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// A DLL's log: its state lives in the value, which the DLL keeps in a `static`.
pub struct Logger {
    file: &'static str,
    state: Mutex<Option<State>>,
}

#[cfg(windows)]
fn wine_output() -> Option<WineOutput> {
    unsafe extern "system" {
        fn GetModuleHandleA(name: *const u8) -> *mut core::ffi::c_void;
        fn GetProcAddress(
            module: *mut core::ffi::c_void,
            name: *const u8,
        ) -> *mut core::ffi::c_void;
    }
    // SAFETY: the name is a NUL-terminated literal; a missing module gives null, not a crash.
    let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
    // SAFETY: the name is a NUL-terminated literal; a null module makes GetProcAddress fail, not crash.
    let address = unsafe { GetProcAddress(ntdll, c"__wine_dbg_output".as_ptr().cast()) };
    // SAFETY: ntdll's `__wine_dbg_output` has `WineOutput`'s signature.
    (!address.is_null())
        .then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, WineOutput>(address) })
}

#[cfg(not(windows))]
const fn wine_output() -> Option<WineOutput> {
    None
}

impl Logger {
    /// A log that writes `<exe base>_<file>` (`file` is `nvngx.log`, say) once it has a message.
    #[must_use]
    pub const fn new(file: &'static str) -> Self {
        Self {
            file,
            state: Mutex::new(None),
        }
    }

    /// Writes `message` at `level` if the log's level lets it through.
    fn emit(&self, level: &Level, message: &str) {
        self.emit_with(&process_env, level, message);
    }

    /// Writes a message below warnings, which the performance build (`perf-build`) leaves out.
    fn below_warn(&self, env: &impl Fn(&str) -> Option<String>, level: &Level, message: &str) {
        if cfg!(not(feature = "perf-build")) {
            self.emit_with(env, level, message);
        }
    }

    /// `emit` with the environment passed in, so a host test can steer it.
    ///
    /// As `Logger::emitMsg`, the file is created by the first message that passes the level.
    fn emit_with(&self, env: &impl Fn(&str) -> Option<String>, level: &Level, message: &str) {
        let Ok(mut guard) = self.state.lock() else {
            return;
        };
        let state = guard.get_or_insert_with(|| State {
            level: level_from(env("DXMT_LOG_LEVEL").as_deref()),
            sinks: None,
        });
        if *level < state.level {
            return;
        }
        let sinks = state
            .sinks
            .get_or_insert_with(|| Sinks::open(self.file, env));
        let text = lines(level, message);
        if let Some(wine) = sinks.wine {
            if let Ok(text) = std::ffi::CString::new(text.as_str()) {
                // SAFETY: `text` is NUL-terminated and outlives the call.
                unsafe { wine(text.as_ptr()) };
            }
        } else {
            let _ = std::io::stderr().write_all(text_mode(&text, cfg!(windows)).as_bytes());
        }
        if let Some(file) = sinks.file.as_mut() {
            let _ = file.write_all(text_mode(&text, cfg!(windows)).as_bytes());
        }
    }

    /// Logs an error.
    pub fn err(&self, message: &str) {
        self.emit(&Level::Error, message);
    }

    /// Logs a warning.
    pub fn warn(&self, message: &str) {
        self.emit(&Level::Warn, message);
    }

    /// Logs at info level (nothing in the performance build).
    pub fn info(&self, message: &str) {
        self.below_warn(&process_env, &Level::Info, message);
    }

    /// Logs at debug level (nothing in the performance build).
    pub fn debug(&self, message: &str) {
        self.below_warn(&process_env, &Level::Debug, message);
    }

    /// Logs at trace level (nothing in the performance build).
    pub fn trace(&self, message: &str) {
        self.below_warn(&process_env, &Level::Trace, message);
    }
}

/// Whether a message at `level` is formatted at all.
///
/// The performance build (`perf-build`, compiled into this crate) drops everything below
/// warnings, so its callers' `format!` never runs.
/// The log macros ([`err!`](crate::err) and its siblings) ask it, so the gate is this crate's
/// feature and not the calling crate's.
#[must_use]
#[inline]
pub const fn enabled(level: &Level) -> bool {
    cfg!(not(feature = "perf-build")) || matches!(level, Level::Warn | Level::Error | Level::None)
}

/// Logs an error to `$log`, a DLL's [`Logger`], with `format!`'s arguments.
#[macro_export]
macro_rules! err {
    ($log:expr, $($arg:tt)*) => {
        if $crate::log::enabled(&$crate::log::Level::Error) {
            $log.err(&::std::format!($($arg)*));
        }
    };
}

/// Logs a warning to `$log`, a DLL's [`Logger`], with `format!`'s arguments.
#[macro_export]
macro_rules! warn {
    ($log:expr, $($arg:tt)*) => {
        if $crate::log::enabled(&$crate::log::Level::Warn) {
            $log.warn(&::std::format!($($arg)*));
        }
    };
}

/// Logs at info level to `$log`; the performance build formats nothing.
#[macro_export]
macro_rules! info {
    ($log:expr, $($arg:tt)*) => {
        if $crate::log::enabled(&$crate::log::Level::Info) {
            $log.info(&::std::format!($($arg)*));
        }
    };
}

/// Logs at debug level to `$log`; the performance build formats nothing.
#[macro_export]
macro_rules! debug {
    ($log:expr, $($arg:tt)*) => {
        if $crate::log::enabled(&$crate::log::Level::Debug) {
            $log.debug(&::std::format!($($arg)*));
        }
    };
}

/// Logs at trace level to `$log`; the performance build formats nothing.
#[macro_export]
macro_rules! trace {
    ($log:expr, $($arg:tt)*) => {
        if $crate::log::enabled(&$crate::log::Level::Trace) {
            $log.trace(&::std::format!($($arg)*));
        }
    };
}

/// Logs an error to `$log` the first time this call site runs, and never again.
///
/// The latch is a per-call-site `static` that carries nothing beyond "fired".
#[macro_export]
macro_rules! err_once {
    ($log:expr, $($arg:tt)*) => {{
        static LOGGED: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(false);
        if !LOGGED.load(::core::sync::atomic::Ordering::Relaxed)
            && !LOGGED.swap(true, ::core::sync::atomic::Ordering::Relaxed)
        {
            $crate::err!($log, $($arg)*);
        }
    }};
}

mod once;

#[cfg(test)]
mod tests;
