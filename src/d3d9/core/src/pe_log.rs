//! Cross-platform PE-side log file state machine (`runtime.mode = core`).
//!
//! Buffers early lines up to 1 MiB before `Direct3DCreate9` opens the log directory, creates the
//! file lazily on the first written line, prunes old `*.log` files beyond the retention limit, and
//! writes lines unbuffered without spawning a background thread or logging while holding its lock.

use std::{
    fs::{File, OpenOptions},
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Maximum number of log files retained in the log directory, matching the Unix sink.
pub const KEEP: usize = 10;

/// Maximum bytes buffered while pending before early lines are dropped.
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
    /// Creating or flushing the file failed; caller must flush `buffered` to stderr.
    Stderr {
        /// Failure reason suitable for a warning line.
        why: String,
        /// Buffered pre-open bytes to write to stderr outside any lock.
        buffered: Vec<u8>,
        /// Whether [`TRUNCATED_NOTE`] must follow `buffered`.
        truncated: bool,
    },
    /// `open` had already run for this [`PeLog`] instance and did nothing.
    AlreadyOpen,
}

/// Result of calling [`PeLog::write_line`].
#[derive(Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    /// Line written (or buffered/dropped) without needing a stderr write from the caller.
    Ok,
    /// First lazy write created this log file.
    Opened(PathBuf),
    /// Log is already in stderr fallback mode; caller must write the line to stderr.
    WriteStderr,
    /// File creation or write failed and fell back to stderr; caller writes warning and line.
    FellBackToStderr(String),
}

/// Result of calling [`PeLog::materialize_for_panic`].
#[derive(Debug, PartialEq, Eq)]
pub enum PanicFlush {
    /// Pending lines moved to stderr fallback; caller must flush them outside any lock.
    Stderr {
        /// Buffered pre-open bytes to write to stderr.
        buf: Vec<u8>,
        /// Whether [`TRUNCATED_NOTE`] must follow `buf`.
        truncated: bool,
    },
    /// Lazy state created the log file so panic lines can write to its handle.
    Opened,
    /// Lazy file creation failed and transitioned to stderr; caller writes warning outside locks.
    Failed(String),
    /// Already open, in stderr fallback, or closed; nothing to materialize.
    Nothing,
}

enum State {
    Pending {
        buf: Vec<u8>,
        truncated: bool,
    },
    Lazy {
        dir: PathBuf,
        stem: String,
        pid: u32,
    },
    Open {
        file: File,
        path: PathBuf,
    },
    Stderr,
    Closed,
}

/// Process-wide PE log destination state machine.
pub struct PeLog {
    state: State,
    retired: Vec<File>,
}

impl Default for PeLog {
    fn default() -> Self {
        Self::new()
    }
}

impl PeLog {
    /// Creates a new [`PeLog`] in the pending state with an empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: State::Pending {
                buf: Vec::new(),
                truncated: false,
            },
            retired: Vec::new(),
        }
    }

    /// Names `dir`, `stem`, and `pid`, flushing early lines or deferring file creation.
    ///
    /// Idempotent once the pending state has already been left; never emits `log::*!` lines or
    /// touches `stderr` while called so callers holding a mutex cannot deadlock.
    pub fn open(&mut self, dir: PathBuf, stem: String, pid: u32) -> OpenOutcome {
        if !matches!(self.state, State::Pending { .. }) {
            return OpenOutcome::AlreadyOpen;
        }
        let State::Pending { buf, truncated } = core::mem::replace(&mut self.state, State::Stderr)
        else {
            unreachable!("checked Pending variant above");
        };
        if buf.is_empty() && !truncated {
            self.state = State::Lazy { dir, stem, pid };
            return OpenOutcome::Deferred;
        }
        match create_log_file(&dir, &stem, pid, &buf, truncated) {
            Ok((file, path)) => {
                self.state = State::Open {
                    file,
                    path: path.clone(),
                };
                OpenOutcome::Opened(path)
            }
            Err(why) => {
                self.state = State::Stderr;
                OpenOutcome::Stderr {
                    why,
                    buffered: buf,
                    truncated,
                }
            }
        }
    }

    /// Writes `bytes` according to the current state without locking `stderr`.
    ///
    /// Materializes a lazy file on the first non-empty write, retires any failed [`File`] handle
    /// until [`Self::close`], and tells callers when to write to `stderr` outside their lock.
    pub fn write_line(&mut self, bytes: &[u8]) -> WriteOutcome {
        match &mut self.state {
            State::Pending { buf, truncated } => {
                if !*truncated && buf.len().saturating_add(bytes.len()) <= PENDING_CAP {
                    buf.extend_from_slice(bytes);
                } else {
                    *truncated = true;
                }
                WriteOutcome::Ok
            }
            State::Lazy { .. } => {
                if bytes.is_empty() {
                    return WriteOutcome::Ok;
                }
                let State::Lazy { dir, stem, pid } =
                    core::mem::replace(&mut self.state, State::Stderr)
                else {
                    unreachable!("matched Lazy variant above");
                };
                match create_log_file(&dir, &stem, pid, bytes, false) {
                    Ok((file, path)) => {
                        self.state = State::Open {
                            file,
                            path: path.clone(),
                        };
                        WriteOutcome::Opened(path)
                    }
                    Err(why) => {
                        self.state = State::Stderr;
                        WriteOutcome::FellBackToStderr(why)
                    }
                }
            }
            State::Open { file, path } => {
                if let Err(err) = file.write_all(bytes) {
                    let why = format!("{}: {err}", path.display());
                    let prev = core::mem::replace(&mut self.state, State::Stderr);
                    if let State::Open { file: old_file, .. } = prev {
                        self.retired.push(old_file);
                    }
                    return WriteOutcome::FellBackToStderr(why);
                }
                WriteOutcome::Ok
            }
            State::Stderr => WriteOutcome::WriteStderr,
            State::Closed => WriteOutcome::Ok,
        }
    }

    /// Materializes a pending or lazy destination ahead of a panic report.
    ///
    /// Never writes to `stderr` directly; callers flush returned [`PanicFlush::Stderr`] or
    /// [`PanicFlush::Failed`] payloads after releasing any lock.
    pub fn materialize_for_panic(&mut self) -> PanicFlush {
        match self.state {
            State::Pending { .. } => {
                let State::Pending { buf, truncated } =
                    core::mem::replace(&mut self.state, State::Stderr)
                else {
                    unreachable!("matched Pending variant above");
                };
                PanicFlush::Stderr { buf, truncated }
            }
            State::Lazy { .. } => {
                let State::Lazy { dir, stem, pid } =
                    core::mem::replace(&mut self.state, State::Stderr)
                else {
                    unreachable!("matched Lazy variant above");
                };
                match create_log_file(&dir, &stem, pid, b"", false) {
                    Ok((file, path)) => {
                        self.state = State::Open { file, path };
                        PanicFlush::Opened
                    }
                    Err(why) => {
                        self.state = State::Stderr;
                        PanicFlush::Failed(why)
                    }
                }
            }
            State::Open { .. } | State::Stderr | State::Closed => PanicFlush::Nothing,
        }
    }

    /// Drains any buffered pending bytes and switches to closed.
    ///
    /// Used when `runtime.mode = mtld3d` hands logging over to `mtld3d.so`.
    pub fn take_pending_and_close(&mut self) -> Option<(Vec<u8>, bool)> {
        let prev = core::mem::replace(&mut self.state, State::Closed);
        match prev {
            State::Pending { buf, truncated } => Some((buf, truncated)),
            State::Open { file, .. } => {
                self.retired.push(file);
                None
            }
            _ => None,
        }
    }

    /// Closes any active or retired file handles and stops future PE-file writes.
    pub fn close(&mut self) {
        self.state = State::Closed;
        self.retired.clear();
    }

    /// Switches to stderr mode and returns any buffered pending lines to flush outside locks.
    pub fn fall_back_to_stderr(&mut self) -> Option<(Vec<u8>, bool)> {
        let prev = core::mem::replace(&mut self.state, State::Stderr);
        match prev {
            State::Pending { buf, truncated } if !buf.is_empty() || truncated => {
                Some((buf, truncated))
            }
            State::Open { file, .. } => {
                self.retired.push(file);
                None
            }
            _ => None,
        }
    }

    /// Returns the active log [`File`] when open.
    #[must_use]
    pub const fn file(&self) -> Option<&File> {
        match &self.state {
            State::Open { file, .. } => Some(file),
            _ => None,
        }
    }

    /// Returns the active log file path when open.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.state {
            State::Open { path, .. } => Some(path.as_path()),
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
    if !initial.is_empty() {
        file.write_all(initial)
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
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

#[cfg(test)]
mod tests;
