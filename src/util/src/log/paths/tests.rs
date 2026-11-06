//! File names of one process's log and traces.
//!
//! They share the `<stem>-<pid>` prefix, so a process's traces sort next to its log; the
//! trace index is the only difference.

use super::{is_stats_file_name, log_file_name, stats_file_name, trace_file_name};

#[test]
fn log_and_trace_names_share_the_process_prefix() {
    assert_eq!(log_file_name("hl2", 1234), "hl2-1234.log");
    assert_eq!(trace_file_name("hl2", 1234, 1), "hl2-1234-1.gputrace");
    assert!(trace_file_name("hl2", 1234, 7).starts_with("hl2-1234-"));
}

#[test]
fn stats_file_sits_beside_the_log() {
    assert_eq!(stats_file_name("game", 42), "game-42.stats.txt");
}

#[test]
fn only_our_stats_files_match() {
    assert!(is_stats_file_name("game-42.stats.txt"));
    for name in [
        "readme.txt",
        "notes.stats.txt",
        "game-.stats.txt",
        "-42.stats.txt",
        "game-42.log",
    ] {
        assert!(!is_stats_file_name(name), "{name}");
    }
}
