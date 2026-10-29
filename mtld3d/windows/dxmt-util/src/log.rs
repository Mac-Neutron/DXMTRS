//! DXMT's log, written the way `util/log/log.cpp` writes it.
//!
//! The level comes from `DXMT_LOG_LEVEL` (`info` unless it names another). Each line of a message
//! gets the level's prefix and goes to Wine's debug output (stderr outside Wine) and to
//! `<DXMT_LOG_PATH>/<exe>_<dll>.log`: no file when the path is `none`, or unset while Wine's output
//! exists.

use std::{
    fmt::Write as _,
    fs::File,
    io::Write,
    sync::Mutex,
};

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
    message.split_terminator('\n').fold(String::new(), |mut text, line| {
        let _ = writeln!(text, "{prefix}{line}");
        text
    })
}

/// `text` as a text-mode stream writes it.
///
/// On Windows `std::ofstream` and `std::cerr` turn each `\n` into `\r\n`; Wine's debug output
/// takes the `\n` form.
fn text_mode(text: &str, crlf: bool) -> String {
    if crlf { text.replace('\n', "\r\n") } else { text.to_owned() }
}

/// The log file's path: `<log_path>/<exe's base name>_<file>`.
fn file_name(log_path: Option<&str>, wine_output: bool, exe_path: &str, file: &str) -> Option<String> {
    let path = log_path.unwrap_or("");
    if path == "none" || (path.is_empty() && wine_output) {
        return None;
    }
    let exe = exe_path.rsplit(['\\', '/']).next().unwrap_or(exe_path);
    let base = match exe.split_at_checked(exe.len().saturating_sub(4)) {
        Some((stem, ext)) if ext.eq_ignore_ascii_case(".exe") => stem,
        _ => exe,
    };
    let slash = if path.is_empty() || path.ends_with('/') { "" } else { "/" };
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
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let path = env("DXMT_LOG_PATH");
        let file = file_name(path.as_deref(), wine.is_some(), &exe, file).and_then(|name| File::create(name).ok());
        Self { wine, file }
    }
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
        fn GetProcAddress(module: *mut core::ffi::c_void, name: *const u8) -> *mut core::ffi::c_void;
    }
    // SAFETY: the name is a NUL-terminated literal; a missing module gives null, not a crash.
    let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
    // SAFETY: the name is a NUL-terminated literal; a null module makes GetProcAddress fail, not crash.
    let address = unsafe { GetProcAddress(ntdll, c"__wine_dbg_output".as_ptr().cast()) };
    // SAFETY: ntdll's `__wine_dbg_output` has `WineOutput`'s signature.
    (!address.is_null()).then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, WineOutput>(address) })
}

#[cfg(not(windows))]
const fn wine_output() -> Option<WineOutput> {
    None
}

impl Logger {
    /// A log that writes `<exe base>_<file>` (`file` is `nvngx.log`, say) once it has a message.
    #[must_use]
    pub const fn new(file: &'static str) -> Self {
        Self { file, state: Mutex::new(None) }
    }

    /// Writes `message` at `level` if the log's level lets it through.
    fn emit(&self, level: &Level, message: &str) {
        self.emit_with(&|name| std::env::var(name).ok(), level, message);
    }

    /// `emit` with the environment passed in, so a host test can steer it.
    ///
    /// As `Logger::emitMsg`, the file is created by the first message that passes the level.
    fn emit_with(&self, env: &impl Fn(&str) -> Option<String>, level: &Level, message: &str) {
        let Ok(mut guard) = self.state.lock() else { return };
        let state = guard.get_or_insert_with(|| State { level: level_from(env("DXMT_LOG_LEVEL").as_deref()), sinks: None });
        if *level < state.level {
            return;
        }
        let sinks = state.sinks.get_or_insert_with(|| Sinks::open(self.file, env));
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
}

#[cfg(test)]
mod tests;
