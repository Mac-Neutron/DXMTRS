use super::{Level, file_name, level_from, lines, text_mode};

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
    assert_eq!(file_name(Some("/tmp/x"), false, "C:\\game.exe", "nvapi.log").as_deref(), Some("/tmp/x/game_nvapi.log"));
}

#[test]
fn file_names() {
    let exe = "C:\\games\\Game.EXE";
    assert_eq!(file_name(Some("none"), false, exe, "nvngx.log"), None);
    assert_eq!(file_name(None, true, exe, "nvngx.log"), None);
    assert_eq!(file_name(Some(""), true, exe, "nvngx.log"), None);
    assert_eq!(file_name(None, false, exe, "nvngx.log").as_deref(), Some("Game_nvngx.log"));
    assert_eq!(file_name(Some("/tmp/logs"), true, exe, "nvngx.log").as_deref(), Some("/tmp/logs/Game_nvngx.log"));
    assert_eq!(file_name(Some("/tmp/logs/"), true, exe, "nvngx.log").as_deref(), Some("/tmp/logs/Game_nvngx.log"));
    assert_eq!(file_name(Some("/l"), true, "tool", "nvngx.log").as_deref(), Some("/l/tool_nvngx.log"));
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
    assert_eq!(file_name(Some("/l"), true, "C:\\a\u{20ac}bc", "nvngx.log").as_deref(), Some("/l/a\u{20ac}bc_nvngx.log"));
    assert_eq!(file_name(Some("/l"), true, "g\u{20ac}.EXE", "nvngx.log").as_deref(), Some("/l/g\u{20ac}_nvngx.log"));
}
