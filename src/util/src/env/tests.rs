use super::{base_of, cache_name, name_of, path_of};

#[test]
fn base_name_strips_exe() {
    assert_eq!(base_of(name_of("C:\\g\\A.EXE")), "A");
    assert_eq!(base_of(name_of("C:\\g\\Game.exe")), "Game");
    assert_eq!(base_of("a.b.Exe"), "a.b");
    assert_eq!(base_of("tool"), "tool");
    assert_eq!(base_of("a.dll"), "a.dll");
    assert_eq!(base_of("a.exe2"), "a.exe2");
}

/// `matchFileExtension("exe")` takes any prefix of `exe` after the last dot.
#[test]
fn base_name_strips_what_the_cpp_strips() {
    assert_eq!(base_of("a.ex"), "a");
    assert_eq!(base_of("a.E"), "a");
    assert_eq!(base_of("a."), "a");
    assert_eq!(base_of(".exe"), "");
    assert_eq!(base_of("a.xe"), "a.xe");
}

#[test]
fn name_is_after_the_last_backslash() {
    assert_eq!(name_of("C:\\games\\A.exe"), "A.exe");
    assert_eq!(name_of("A.exe"), "A.exe");
    assert_eq!(name_of("C:\\games\\"), "");
    assert_eq!(name_of(""), "");
}

#[test]
fn cache_name_prefers_dxmt_cache_exe() {
    assert_eq!(
        cache_name(Some("replay.exe".to_owned()), "A.exe"),
        "replay.exe"
    );
    assert_eq!(cache_name(Some(String::new()), "A.exe"), "A.exe");
    assert_eq!(cache_name(None, "A.exe"), "A.exe");
}

/// `GetModuleFileNameW` is asked for 260 units; the C++ reads the string up to its NUL.
#[test]
fn a_path_ends_at_its_length_or_its_nul() {
    let units: Vec<u16> = "C:\\g\\A.exe".encode_utf16().collect();
    assert_eq!(path_of(&units), "C:\\g\\A.exe");
    let mut padded = units;
    padded.extend([0, 0x41]);
    assert_eq!(path_of(&padded), "C:\\g\\A.exe");
    assert_eq!(path_of(&[]), "");
}
