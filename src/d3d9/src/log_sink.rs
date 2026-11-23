//! The PE-side logger's sink, backed by [`mtld3d_core::pe_log::PeLog`].
//!
//! `env_logger` hands every formatted line to [`Sink::write`], which buffers lines inside `PeLog`
//! until [`open`] opens the log file on disk (or falls back to stderr). Crash handlers write
//! synchronously through [`write_crash`] and [`write_fault`] without allocating or locking when a
//! Win32 file handle is already open, and [`forward_parked`] flushes buffered lines on panic.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicIsize, Ordering},
};
use std::{
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    sync::Mutex,
};

use log::warn;
use mtld3d_core::pe_log::{OpenOutcome, PanicFlush, PeLog, TRUNCATED_NOTE, WriteOutcome};

use crate::LOG_TARGET;

unsafe extern "system" {
    fn RtlGetCurrentPeb() -> *mut c_void;
    fn RtlIsCriticalSectionLockedByThread(section: *mut c_void) -> i32;
    fn GetStdHandle(std_handle: u32) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        bytes_to_write: u32,
        bytes_written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

/// The log directory beside the executable when `log.dir` is empty.
const DEFAULT_DIR: &str = "mtld3d-logs";

/// Offset of `PEB.LoaderLock`, the loader's `RTL_CRITICAL_SECTION*`, in a 32-bit PEB.
#[cfg(target_pointer_width = "32")]
const PEB_LOADER_LOCK_OFFSET: usize = 0xa0;
/// Offset of `PEB.LoaderLock`, the loader's `RTL_CRITICAL_SECTION*`, in a 64-bit PEB.
#[cfg(target_pointer_width = "64")]
const PEB_LOADER_LOCK_OFFSET: usize = 0x110;

const INVALID_LOG_HANDLE: isize = -1;
const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;

static PE_LOG: Mutex<PeLog> = Mutex::new(PeLog::new());
static LOG_HANDLE: AtomicIsize = AtomicIsize::new(INVALID_LOG_HANDLE);

/// Opens the PE-side log file for the resolved `mtld3d.conf` directory.
pub fn open(cfg: &mtld3d_core::config::Mtld3dConfig) {
    let location = core_log_location(cfg);
    let (dir, stem) = match location {
        Ok(pair) => pair,
        Err(why) => {
            let buffered = with_pe_log(|pe| {
                let buffered = pe.fall_back_to_stderr();
                LOG_HANDLE.store(INVALID_LOG_HANDLE, Ordering::Release);
                buffered
            });
            if let Some((buf, truncated)) = buffered {
                write_stderr_bytes(&buf, truncated);
            }
            warn!(target: LOG_TARGET, "log file: {why}, logging to stderr");
            return;
        }
    };
    let outcome = with_pe_log(|pe| {
        let outcome = pe.open(dir, stem, std::process::id());
        sync_log_handle(pe);
        outcome
    });
    match outcome {
        OpenOutcome::Opened(path) => {
            let _ = &path;
            log::info!(target: LOG_TARGET, "log file: {}", path.display());
        }
        OpenOutcome::Stderr {
            why,
            buffered,
            truncated,
        } => {
            write_stderr_bytes(&buffered, truncated);
            warn!(target: LOG_TARGET, "log file: {why}, logging to stderr");
        }
        OpenOutcome::Deferred | OpenOutcome::AlreadyOpen => {}
    }
}

fn core_log_location(cfg: &mtld3d_core::config::Mtld3dConfig) -> Result<(PathBuf, String), String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe() unavailable ({e})"))?;
    let exe_dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let dir = if cfg.log_dir.is_empty() {
        exe_dir.join(DEFAULT_DIR)
    } else if cfg.log_dir.starts_with('/') && !cfg.log_dir.starts_with("//") {
        PathBuf::from(format!("Z:{}", cfg.log_dir))
    } else {
        exe_dir.join(&cfg.log_dir)
    };
    let stem = exe.file_stem().map_or_else(
        || String::from("mtld3d"),
        |s| s.to_string_lossy().into_owned(),
    );
    Ok((dir, stem))
}

fn with_pe_log<R>(f: impl FnOnce(&mut PeLog) -> R) -> R {
    let mut pe = PE_LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = f(&mut pe);
    drop(pe);
    result
}

fn sync_log_handle(pe: &PeLog) {
    let raw = pe
        .file()
        .map_or(INVALID_LOG_HANDLE, |file| file.as_raw_handle() as isize);
    LOG_HANDLE.store(raw, Ordering::Release);
}

fn write_stderr_bytes(bytes: &[u8], truncated: bool) {
    let mut err = std::io::stderr().lock();
    let _ = std::io::Write::write_all(&mut err, bytes);
    if truncated {
        let _ = std::io::Write::write_all(&mut err, TRUNCATED_NOTE);
    }
}

fn write_stderr_fallback(why: &str, bytes: &[u8]) {
    let mut err = std::io::stderr().lock();
    let _ = std::io::Write::write_all(
        &mut err,
        format!("[mtld3d::d3d9] log file: {why}, logging to stderr\n").as_bytes(),
    );
    let _ = std::io::Write::write_all(&mut err, bytes);
}

/// Closes any open PE log handle when `d3d9.dll` unloads unused after `veh::uninstall()`.
pub fn close_on_unload() {
    LOG_HANDLE.store(INVALID_LOG_HANDLE, Ordering::Release);
    if let Ok(mut pe) = PE_LOG.try_lock() {
        pe.close();
    }
}

/// The `env_logger` target: writes each formatted line into [`PE_LOG`].
pub struct Sink;

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let outcome = with_pe_log(|pe| {
            let outcome = pe.write_line(buf);
            if matches!(
                outcome,
                WriteOutcome::Opened(_) | WriteOutcome::FellBackToStderr(_)
            ) {
                sync_log_handle(pe);
            }
            outcome
        });
        match outcome {
            WriteOutcome::Ok | WriteOutcome::Opened(_) => {}
            WriteOutcome::WriteStderr => write_stderr_bytes(buf, false),
            WriteOutcome::FellBackToStderr(why) => write_stderr_fallback(&why, buf),
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Whether the calling thread holds the loader lock, read off the process's PEB.
pub fn caller_holds_loader_lock() -> bool {
    // SAFETY: ntdll export; answers this process's PEB, which lives as long as
    // the process.
    let peb = unsafe { RtlGetCurrentPeb() };
    if peb.is_null() {
        return false;
    }
    // SAFETY: the offset is `PEB.LoaderLock`'s on this architecture, inside the PEB.
    let field = unsafe { peb.cast::<u8>().add(PEB_LOADER_LOCK_OFFSET) };
    // SAFETY: the field is a pointer the loader set at process start, readable
    // for the process's lifetime; `read_unaligned` asks nothing of its alignment.
    let section = unsafe { field.cast::<*mut c_void>().read_unaligned() };
    if section.is_null() {
        return false;
    }
    // SAFETY: ntdll export; `section` is the loader's live critical section.
    unsafe { RtlIsCriticalSectionLockedByThread(section) != 0 }
}

/// Write a line of a terminal fault's report synchronously on the calling thread.
///
/// Writes to the open log file handle if available, or directly to stderr without allocating or
/// taking locks.
pub fn write_crash(line: &[u8]) {
    let raw = LOG_HANDLE.load(Ordering::Acquire);
    if is_valid_handle(raw) {
        write_all_handle(raw as *mut c_void, line);
    } else {
        // SAFETY: Win32 query for the process stderr handle; lock-free and allocation-free.
        let stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        if !stderr.is_null() && is_valid_handle(stderr as isize) {
            write_all_handle(stderr, line);
        }
    }
}

/// Write a line of a first-chance fault report synchronously to the open log handle.
pub fn write_fault(line: &[u8]) {
    let raw = LOG_HANDLE.load(Ordering::Acquire);
    if is_valid_handle(raw) {
        write_all_handle(raw as *mut c_void, line);
    }
}

const fn is_valid_handle(raw: isize) -> bool {
    raw != 0 && raw != INVALID_LOG_HANDLE
}

fn write_all_handle(handle: *mut c_void, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let chunk = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let mut written = 0_u32;
        // SAFETY: `handle` is an open Win32 file/stderr handle, `bytes` is readable for `chunk`
        // bytes, and `written` is a writable stack local for synchronous `WriteFile`.
        let ok = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr().cast::<c_void>(),
                chunk,
                &raw mut written,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 || written == 0 {
            return;
        }
        bytes = &bytes[usize::try_from(written)
            .unwrap_or(bytes.len())
            .min(bytes.len())..];
    }
}

/// Materialize buffered early log lines ahead of a panic report.
pub fn forward_parked() {
    let flush = PE_LOG.try_lock().ok().map(|mut pe| {
        let action = pe.materialize_for_panic();
        sync_log_handle(&pe);
        action
    });
    if let Some(action) = flush {
        match action {
            PanicFlush::Stderr { buf, truncated } => write_stderr_bytes(&buf, truncated),
            PanicFlush::Failed(why) => write_stderr_fallback(&why, b""),
            PanicFlush::Opened | PanicFlush::Nothing => {}
        }
    }
}
