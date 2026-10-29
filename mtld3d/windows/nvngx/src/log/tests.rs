use super::{Level, file_name, level_from, lines};

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
    assert_eq!(lines(Level::Warn, "a\nb"), "warn:  a\nwarn:  b\n");
    assert_eq!(lines(Level::Error, "x\n"), "err:   x\n");
    assert_eq!(lines(Level::Trace, "t"), "trace: t\n");
    assert_eq!(lines(Level::Debug, "d"), "debug: d\n");
    assert_eq!(lines(Level::Info, "i"), "info:  i\n");
    assert_eq!(lines(Level::Info, ""), "");
    assert_eq!(lines(Level::Info, "\n"), "info:  \n");
}

#[test]
fn file_names() {
    let exe = "C:\\games\\Game.EXE";
    assert_eq!(file_name(Some("none"), false, exe), None);
    assert_eq!(file_name(None, true, exe), None);
    assert_eq!(file_name(Some(""), true, exe), None);
    assert_eq!(file_name(None, false, exe).as_deref(), Some("Game_nvngx.log"));
    assert_eq!(file_name(Some("/tmp/logs"), true, exe).as_deref(), Some("/tmp/logs/Game_nvngx.log"));
    assert_eq!(file_name(Some("/tmp/logs/"), true, exe).as_deref(), Some("/tmp/logs/Game_nvngx.log"));
    assert_eq!(file_name(Some("/l"), true, "tool").as_deref(), Some("/l/tool_nvngx.log"));
}
