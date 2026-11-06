//! Names of the files one process writes into its log directory.
//!
//! Both sides agree on them: the PE side names the directory, the unix side
//! opens the files. One log per process, so a launch never overwrites the
//! log of the one before it, and the GPU traces of a process sit next to
//! its log under the same prefix. `pid` is the host (macOS) process id, not
//! Wine's: Wine numbers its processes per wineserver and hands the first one
//! the same id every launch, so it cannot tell two runs apart.

/// The log file of process `pid` running executable `stem`: `<stem>-<pid>.log`.
#[must_use]
pub fn log_file_name(stem: &str, pid: u32) -> String {
    format!("{stem}-{pid}.log")
}

/// The `index`-th GPU trace of that process: `<stem>-<pid>-<index>.gputrace`.
#[must_use]
pub fn trace_file_name(stem: &str, pid: u32, index: u32) -> String {
    format!("{stem}-{pid}-{index}.gputrace")
}

/// The statistics of that process (D3D9's perf-tracking builds): `<stem>-<pid>.stats.txt`.
#[must_use]
pub fn stats_file_name(stem: &str, pid: u32) -> String {
    format!("{stem}-{pid}.stats.txt")
}

/// Whether `name` has [`stats_file_name`]'s shape: a stem, `-`, ASCII digits, `.stats.txt`.
#[must_use]
pub fn is_stats_file_name(name: &str) -> bool {
    name.strip_suffix(".stats.txt")
        .and_then(|rest| rest.rsplit_once('-'))
        .is_some_and(|(stem, pid)| {
            !stem.is_empty() && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests;
