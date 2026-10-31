use super::{Level, Logger, file_name, level_from, lines, text_mode};

#[test]
fn level_names_and_default() {
    assert!(matches!(level_from(Some("trace")), Level::Trace));
    assert!(matches!(level_from(Some("debug")), Level::Debug));
    assert!(matches!(level_from(Some("info")), Level::Info));
    assert!(matches!(level_from(Some("warn")), Level::Warn));
    assert!(matches!(level_from(Some("error")), Level::Error));
    assert!(matches!(level_from(Some("none")), Level::None));
    assert!(matches!(level_from(Some("verbose")), Level::Info));
    assert!(matches!(level_from(None), Level::Info));
}

#[test]
fn each_line_prefixed() {
    assert_eq!(lines(&Level::Warn, "a\nb"), "warn:  a\nwarn:  b\n");
    assert_eq!(lines(&Level::Error, "x\n"), "err:   x\n");
    assert_eq!(lines(&Level::Trace, "t"), "trace: t\n");
    assert_eq!(lines(&Level::Debug, "d"), "debug: d\n");
    assert_eq!(lines(&Level::Info, "i"), "info:  i\n");
    assert_eq!(lines(&Level::Info, ""), "");
    assert_eq!(lines(&Level::Info, "\n"), "info:  \n");
}

#[test]
fn file_name_names_the_dll() {
    assert_eq!(
        file_name(Some("/tmp/x"), false, "C:\\game.exe", "nvapi.log").as_deref(),
        Some("/tmp/x/game_nvapi.log")
    );
}

#[test]
fn file_names() {
    let exe = "C:\\games\\Game.EXE";
    assert_eq!(file_name(Some("none"), false, exe, "nvngx.log"), None);
    assert_eq!(file_name(None, true, exe, "nvngx.log"), None);
    assert_eq!(file_name(Some(""), true, exe, "nvngx.log"), None);
    assert_eq!(
        file_name(None, false, exe, "nvngx.log").as_deref(),
        Some("Game_nvngx.log")
    );
    assert_eq!(
        file_name(Some("/tmp/logs"), true, exe, "nvngx.log").as_deref(),
        Some("/tmp/logs/Game_nvngx.log")
    );
    assert_eq!(
        file_name(Some("/tmp/logs/"), true, exe, "nvngx.log").as_deref(),
        Some("/tmp/logs/Game_nvngx.log")
    );
    assert_eq!(
        file_name(Some("/l"), true, "tool", "nvngx.log").as_deref(),
        Some("/l/tool_nvngx.log")
    );
}

#[test]
fn text_mode_ends_lines_in_crlf() {
    let text = lines(&Level::Warn, "a\nb");
    assert_eq!(text_mode(&text, true), "warn:  a\r\nwarn:  b\r\n");
    assert_eq!(text_mode(&text, false), text);
}

#[test]
fn a_multibyte_name_without_exe_keeps_its_name() {
    // Its last 4 bytes start inside the euro sign: slicing there panics.
    assert_eq!(
        file_name(Some("/l"), true, "C:\\a\u{20ac}bc", "nvngx.log").as_deref(),
        Some("/l/a\u{20ac}bc_nvngx.log")
    );
    assert_eq!(
        file_name(Some("/l"), true, "g\u{20ac}.EXE", "nvngx.log").as_deref(),
        Some("/l/g\u{20ac}_nvngx.log")
    );
}

/// A fresh directory for a log file.
fn log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dxmt-util-log-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_file_is_created_by_the_first_message_that_passes_the_level() {
    let dir = log_dir("level");
    let path = dir.to_string_lossy().into_owned();
    let env = |level: &'static str| {
        let path = path.clone();
        move |name: &str| match name {
            "DXMT_LOG_LEVEL" => Some(level.to_owned()),
            "DXMT_LOG_PATH" => Some(path.clone()),
            _ => None,
        }
    };
    let filtered = Logger::new("t.log");
    filtered.emit_with(&env("none"), &Level::Error, "dropped");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "a filtered message created the file"
    );
    let passing = Logger::new("t.log");
    passing.emit_with(&env("warn"), &Level::Warn, "kept");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn native_file_name_has_an_empty_exe() {
    assert_eq!(
        file_name(Some("/t"), false, "", "dxgi.log").as_deref(),
        Some("/t/_dxgi.log")
    );
}

/// The environment of a log at `level`, writing into `path`.
fn steered(level: &'static str, path: String) -> impl Fn(&str) -> Option<String> {
    move |name: &str| match name {
        "DXMT_LOG_LEVEL" => Some(level.to_owned()),
        "DXMT_LOG_PATH" => Some(path.clone()),
        _ => None,
    }
}

#[cfg(not(feature = "perf-build"))]
#[test]
fn info_passes_at_info_and_debug_does_not() {
    let dir = log_dir("info");
    let env = steered("info", dir.to_string_lossy().into_owned());
    let debug = Logger::new("t.log");
    debug.below_warn(&env, &Level::Debug, "dropped");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "debug passed at the info level"
    );
    let info = Logger::new("t.log");
    info.below_warn(&env, &Level::Info, "kept");
    let written = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(written, ["info:  kept\n"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "perf-build")]
#[test]
fn the_performance_build_logs_nothing_below_warn() {
    let dir = log_dir("perf");
    let env = steered("trace", dir.to_string_lossy().into_owned());
    let log = Logger::new("t.log");
    for level in [Level::Trace, Level::Debug, Level::Info] {
        log.below_warn(&env, &level, "dropped");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    log.emit_with(&env, &Level::Warn, "kept");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
