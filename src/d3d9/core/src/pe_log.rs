//! Cross-platform PE-side log file state machine (`runtime.mode = core`).
//!
//! Buffers early lines up to 1 MiB before `Direct3DCreate9` opens the log directory, creates the
//! file lazily on the first written line, prunes old `*.log` files beyond the retention limit, and
//! writes lines unbuffered without spawning a background thread or logging while holding its lock.

use std::{
    fs::{File, OpenOptions},
    io::{self, ErrorKind, Write as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Maximum number of log files retained in the log directory, matching the Unix sink.
pub const KEEP: usize = 10;

/// Maximum bytes buffered in [`PeLog::Pending`] before early lines are dropped.
pub const PENDING_CAP: usize = 1024 * 1024;

/// Maximum collision suffix tried when `{stem}-{pid}.log` already exists.
const MAX_COLLISION_SUFFIX: u32 = 1024;

/// Single truncation notice appended when early lines exceed [`PENDING_CAP`].
pub const TRUNCATED_NOTE: &[u8] =
    b"mtld3d: early log buffer full; further lines dropped until log opens\n";

/// Result of calling [`PeLog::open`].
#[derive(Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    /// Early buffered lines existed and were flushed into the newly created file at this path.
    Opened(PathBuf),
    /// No early lines existed yet, so file creation waits for the first non-empty write.
    Deferred,
    /// Creating or flushing the file failed; buffered lines fell back to stderr with this reason.
    Stderr(String),
    /// `open` had already run for this [`PeLog`] instance and did nothing.
    AlreadyOpen,
}

/// Result of calling [`PeLog::write_line`].
#[derive(Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    /// Line written (or dropped on pending overflow) without state transition to stderr.
    Ok,
    /// First lazy write created this log file.
    Opened(PathBuf),
    /// File creation or write failed and fell back to stderr with this reason.
    FellBackToStderr(String),
}

/// Process-wide PE log destination state machine.
pub enum PeLog {
    /// Before `Direct3DCreate9` resolves `log.dir`; holds up to [`PENDING_CAP`] bytes in memory.
    Pending {
        /// Buffered pre-open log bytes.
        buf: Vec<u8>,
        /// True once buffered lines exceeded [`PENDING_CAP`].
        truncated: bool,
    },
    /// `open` ran with no early lines; directory and file wait until the first non-empty line.
    Lazy {
        /// Target log directory (`mtld3d-logs` or `log.dir`).
        dir: PathBuf,
        /// Executable stem used to name `{stem}-{pid}.log`.
        stem: String,
        /// Process identifier embedded in the file name.
        pid: u32,
    },
    /// Active unbuffered log file.
    Open {
        /// Open log file handle.
        file: File,
        /// Resolved path of `file`.
        path: PathBuf,
    },
    /// Permanent fallback to `stderr` after an open or write error.
    Stderr,
    /// Closed / handed off state (no-op on writes).
    Closed,
}

impl Default for PeLog {
    fn default() -> Self {
        Self::new()
    }
}

impl PeLog {
    /// Creates a new [`PeLog`] in [`PeLog::Pending`] with an empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self::Pending {
            buf: Vec::new(),
            truncated: false,
        }
    }

    /// Names `dir`, `stem`, and `pid`, flushing early lines or deferring file creation.
    ///
    /// Idempotent once [`Self::Pending`] has already been left; never emits `log::*!` lines while
    /// called so callers holding a mutex cannot deadlock.
    pub fn open(&mut self, dir: PathBuf, stem: String, pid: u32) -> OpenOutcome {
        if !matches!(self, Self::Pending { .. }) {
            return OpenOutcome::AlreadyOpen;
        }
        let Self::Pending { buf, truncated } = core::mem::replace(self, Self::Stderr) else {
            unreachable!("checked Pending variant above");
        };
        if buf.is_empty() && !truncated {
            *self = Self::Lazy { dir, stem, pid };
            return OpenOutcome::Deferred;
        }
        match create_log_file(&dir, &stem, pid, &buf, truncated) {
            Ok((file, path)) => {
                *self = Self::Open {
                    file,
                    path: path.clone(),
                };
                OpenOutcome::Opened(path)
            }
            Err(why) => {
                *self = Self::Stderr;
                write_stderr(&buf, truncated);
                OpenOutcome::Stderr(why)
            }
        }
    }

    /// Writes `bytes` according to the current state without echoing while a file is open.
    ///
    /// Materializes a [`Self::Lazy`] file on the first non-empty write and falls back to stderr on
    /// I/O errors.
    pub fn write_line(&mut self, bytes: &[u8]) -> WriteOutcome {
        match self {
            Self::Pending { buf, truncated } => {
                if !*truncated && buf.len().saturating_add(bytes.len()) <= PENDING_CAP {
                    buf.extend_from_slice(bytes);
                } else {
                    *truncated = true;
                }
                WriteOutcome::Ok
            }
            Self::Lazy { .. } => {
                if bytes.is_empty() {
                    return WriteOutcome::Ok;
                }
                let Self::Lazy { dir, stem, pid } = core::mem::replace(self, Self::Stderr) else {
                    unreachable!("matched Lazy variant above");
                };
                match create_log_file(&dir, &stem, pid, bytes, false) {
                    Ok((file, path)) => {
                        *self = Self::Open {
                            file,
                            path: path.clone(),
                        };
                        WriteOutcome::Opened(path)
                    }
                    Err(why) => {
                        *self = Self::Stderr;
                        write_stderr(bytes, false);
                        WriteOutcome::FellBackToStderr(why)
                    }
                }
            }
            Self::Open { file, path } => {
                if let Err(err) = file.write_all(bytes) {
                    let why = format!("{}: {err}", path.display());
                    *self = Self::Stderr;
                    write_stderr(bytes, false);
                    return WriteOutcome::FellBackToStderr(why);
                }
                WriteOutcome::Ok
            }
            Self::Stderr => {
                write_stderr(bytes, false);
                WriteOutcome::Ok
            }
            Self::Closed => WriteOutcome::Ok,
        }
    }

    /// Drains any buffered [`Self::Pending`] bytes and switches to [`Self::Closed`].
    ///
    /// Used when `runtime.mode = mtld3d` hands logging over to `mtld3d.so`.
    pub fn take_pending_and_close(&mut self) -> Option<(Vec<u8>, bool)> {
        let prev = core::mem::replace(self, Self::Closed);
        match prev {
            Self::Pending { buf, truncated } => Some((buf, truncated)),
            _ => None,
        }
    }

    /// Closes any open file handle and stops future PE-file writes.
    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Flushes any buffered [`Self::Pending`] lines to stderr and switches to [`Self::Stderr`].
    pub fn fall_back_to_stderr(&mut self) {
        let prev = core::mem::replace(self, Self::Stderr);
        if let Self::Pending { buf, truncated } = prev
            && (!buf.is_empty() || truncated)
        {
            write_stderr(&buf, truncated);
        }
    }

    /// Returns the active log [`File`] when in [`Self::Open`].
    #[must_use]
    pub const fn file(&self) -> Option<&File> {
        match self {
            Self::Open { file, .. } => Some(file),
            _ => None,
        }
    }

    /// Returns the active log file path when in [`Self::Open`].
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Open { path, .. } => Some(path.as_path()),
            _ => None,
        }
    }
}

/// Creates `dir`, prunes `*.log`, opens `{stem}-{pid}[-n].log`, and writes `initial`.
///
/// Prunes regular `*.log` files to `KEEP - 1`, uses `create_new(true)` for collision avoidance, and
/// appends [`TRUNCATED_NOTE`] when `truncated` is set.
fn create_log_file(
    dir: &Path,
    stem: &str,
    pid: u32,
    initial: &[u8],
    truncated: bool,
) -> Result<(File, PathBuf), String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    prune_old_logs(dir, KEEP - 1);
    let (mut file, path) = open_unique_log(dir, stem, pid)?;
    file.write_all(initial)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    if truncated {
        file.write_all(TRUNCATED_NOTE)
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok((file, path))
}

/// Opens `{stem}-{pid}.log` or collision-suffixed `{stem}-{pid}-{2..=1024}.log` exclusively.
fn open_unique_log(dir: &Path, stem: &str, pid: u32) -> Result<(File, PathBuf), String> {
    let base = mtld3d_shared::log_paths::log_file_name(stem, pid);
    let first = dir.join(base);
    match OpenOptions::new().write(true).create_new(true).open(&first) {
        Ok(file) => return Ok((file, first)),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
        Err(err) => return Err(format!("{}: {err}", first.display())),
    }
    for suffix in 2..=MAX_COLLISION_SUFFIX {
        let name = format!("{stem}-{pid}-{suffix}.log");
        let candidate = dir.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((file, candidate)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(format!("{}: {err}", candidate.display())),
        }
    }
    Err(format!(
        "{}: exhausted {MAX_COLLISION_SUFFIX} log name collisions",
        first.display()
    ))
}

/// Deletes the oldest regular `*.log` files in `dir` until at most `keep` remain.
///
/// Directories and symlinks (even with a `.log` extension) are ignored, and no `log::*!` calls
/// occur so callers holding a sink mutex cannot re-enter.
fn prune_old_logs(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut aged: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|ft| ft.is_file()))
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "log") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    if aged.len() <= keep {
        return;
    }
    aged.sort();
    let doomed = aged.len() - keep;
    for (_, path) in aged.into_iter().take(doomed) {
        let _ = std::fs::remove_file(path);
    }
}

/// Best-effort write to `stderr` when no log file could be opened or written.
fn write_stderr(bytes: &[u8], truncated: bool) {
    let mut err = io::stderr().lock();
    let _ = err.write_all(bytes);
    if truncated {
        let _ = err.write_all(TRUNCATED_NOTE);
    }
}

#[cfg(test)]
mod tests;
