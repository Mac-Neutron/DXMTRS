//! DXMT's log, written the way `util/log/log.cpp` writes it.
//!
//! The level comes from `DXMT_LOG_LEVEL` (`info` unless it names another). Each line of a message gets the level's
//! prefix and goes to Wine's debug output (stderr outside Wine) and to `<DXMT_LOG_PATH>/<exe>_nvngx.log`: no file
//! when the path is `none`, or unset while Wine's output exists.

use std::{
    fs::File,
    io::Write,
    sync::{LazyLock, Mutex},
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
fn lines(level: Level, message: &str) -> String {
    let prefix = match level {
        Level::Trace => "trace: ",
        Level::Debug => "debug: ",
        Level::Info => "info:  ",
        Level::Warn => "warn:  ",
        Level::Error | Level::None => "err:   ",
    };
    message.split_terminator('\n').map(|line| format!("{prefix}{line}\n")).collect()
}

/// The log file's path: `<log_path>/<exe's base name>_nvngx.log`.
fn file_name(log_path: Option<&str>, wine_output: bool, exe_path: &str) -> Option<String> {
    let path = log_path.unwrap_or("");
    if path == "none" || (path.is_empty() && wine_output) {
        return None;
    }
    let exe = exe_path.rsplit(['\\', '/']).next().unwrap_or(exe_path);
    let base = match exe.len().checked_sub(4) {
        Some(at) if exe[at..].eq_ignore_ascii_case(".exe") => &exe[..at],
        _ => exe,
    };
    let slash = if path.is_empty() || path.ends_with('/') { "" } else { "/" };
    Some(format!("{path}{slash}{base}_nvngx.log"))
}

/// Wine's `__wine_dbg_output`, which ntdll exports.
type WineOutput = unsafe extern "C" fn(*const core::ffi::c_char) -> i32;

/// The log's state, opened by its first message.
struct Log {
    level: Level,
    wine: Option<WineOutput>,
    file: Option<File>,
}

static LOG: LazyLock<Mutex<Log>> = LazyLock::new(|| {
    let wine = wine_output();
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let path = std::env::var("DXMT_LOG_PATH").ok();
    let file = file_name(path.as_deref(), wine.is_some(), &exe).and_then(|name| File::create(name).ok());
    let level = level_from(std::env::var("DXMT_LOG_LEVEL").ok().as_deref());
    Mutex::new(Log { level, wine, file })
});

#[cfg(windows)]
fn wine_output() -> Option<WineOutput> {
    unsafe extern "system" {
        fn GetModuleHandleA(name: *const u8) -> *mut core::ffi::c_void;
        fn GetProcAddress(module: *mut core::ffi::c_void, name: *const u8) -> *mut core::ffi::c_void;
    }
    // SAFETY: both names are NUL-terminated literals; a null module makes GetProcAddress fail, not crash.
    let address = unsafe { GetProcAddress(GetModuleHandleA(c"ntdll.dll".as_ptr().cast()), c"__wine_dbg_output".as_ptr().cast()) };
    // SAFETY: ntdll's `__wine_dbg_output` has `WineOutput`'s signature.
    (!address.is_null()).then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, WineOutput>(address) })
}

#[cfg(not(windows))]
const fn wine_output() -> Option<WineOutput> {
    None
}

/// Writes `message` at `level` if the log's level lets it through.
fn emit(level: Level, message: &str) {
    let Ok(mut log) = LOG.lock() else { return };
    if level < log.level {
        return;
    }
    let text = lines(level, message);
    if let Some(wine) = log.wine {
        if let Ok(text) = std::ffi::CString::new(text.as_str()) {
            // SAFETY: `text` is NUL-terminated and outlives the call.
            unsafe { wine(text.as_ptr()) };
        }
    } else {
        let _ = std::io::stderr().write_all(text.as_bytes());
    }
    if let Some(file) = log.file.as_mut() {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Logs an error.
pub fn err(message: &str) {
    emit(Level::Error, message);
}

/// Logs a warning.
pub fn warn(message: &str) {
    emit(Level::Warn, message);
}

/// Logs a trace message.
pub fn trace(message: &str) {
    emit(Level::Trace, message);
}

#[cfg(test)]
mod tests;
