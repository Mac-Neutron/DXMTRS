use std::path::PathBuf;

use super::*;

/// An empty scratch folder of its own for `test`, and its name in Wine's form.
fn scratch(test: &str) -> (PathBuf, OsString) {
    let dir = std::env::temp_dir().join(format!("dxmt-dump-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let wine = wine_folder(dir.clone().into_os_string()).unwrap();
    (dir, wine)
}

#[test]
fn save_replaces() {
    let (dir, folder) = scratch("save_replaces");
    save_in(&folder, "stats.txt", b"one");
    save_in(&folder, "stats.txt", b"two, longer");
    assert_eq!(
        std::fs::read(dir.join("stats.txt")).unwrap(),
        b"two, longer"
    );
    assert!(!dir.join("stats.txt.tmp").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_once_keeps_existing() {
    let (dir, folder) = scratch("save_once_keeps_existing");
    save_once_in(&folder, "ps-0123456789abcdef.dxil", b"first");
    save_once_in(&folder, "ps-0123456789abcdef.dxil", b"second");
    assert_eq!(
        std::fs::read(dir.join("ps-0123456789abcdef.dxil")).unwrap(),
        b"first"
    );
    assert!(!dir.join("ps-0123456789abcdef.dxil.tmp").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_write_leaves_no_tmp() {
    let (dir, folder) = scratch("failed_write_leaves_no_tmp");
    // A folder where the file goes: the temporary file is written, its rename fails.
    std::fs::create_dir(dir.join("passes.txt")).unwrap();
    save_in(&folder, "passes.txt", b"data");
    save_once_in(&folder, "draws.txt\\x", b"data");
    assert!(dir.join("passes.txt").is_dir());
    assert!(!dir.join("passes.txt.tmp").exists());
    assert!(!dir.join("draws.txt").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn make_folder_true_when_new_or_off() {
    let (dir, folder) = scratch("make_folder_true_when_new_or_off");
    assert!(make_folder_in(None, "f9-1"));
    assert!(make_folder_in(Some(&folder), "f9-1"));
    assert!(dir.join("f9-1").is_dir());
    assert!(!make_folder_in(Some(&folder), "f9-1"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn make_folder_true_on_other_errors() {
    let (dir, folder) = scratch("make_folder_true_on_other_errors");
    // F9's `while (!MakeCaptureFolder(dir))` must end when the folder cannot be made at all.
    let missing = join(&folder, "missing");
    assert!(make_folder_in(Some(&missing), "f9-1"));
    assert!(!dir.join("missing").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn folder_kept_when_mkdir_fails() {
    let (dir, _) = scratch("folder_kept_when_mkdir_fails");
    let value = dir.join("missing").join("dump").into_os_string();
    let mut expected = OsString::from("Z:");
    expected.push(&value);
    assert_eq!(open(Some(value)), Some(expected));
    assert!(!dir.join("missing").exists());
    assert_eq!(open(None), None);
    assert_eq!(open(Some(OsString::new())), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_into_subfolder_with_backslash() {
    let (dir, folder) = scratch("save_into_subfolder_with_backslash");
    assert!(make_folder_in(Some(&folder), "f9-3"));
    save_in(&folder, "f9-3\\pass-2 render.bin", b"pixels");
    assert_eq!(
        std::fs::read(dir.join("f9-3").join("pass-2 render.bin")).unwrap(),
        b"pixels"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn append_pipeline_adds_newline() {
    let (dir, folder) = scratch("append_pipeline_adds_newline");
    append_in(&folder, b"cs cs-0011223344556677 rs-8899aabbccddeeff");
    append_in(&folder, b"gfx vs-01 ps-02");
    assert_eq!(
        std::fs::read(dir.join("pipelines.txt")).unwrap(),
        b"cs cs-0011223344556677 rs-8899aabbccddeeff\ngfx vs-01 ps-02\n"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mac_path_gets_the_z_drive() {
    assert_eq!(
        wine_folder("/Users/me/dump".into()),
        Some("Z:/Users/me/dump".into())
    );
    assert_eq!(wine_folder("C:\\dump".into()), Some("C:\\dump".into()));
    assert_eq!(wine_folder("rel".into()), Some("rel".into()));
    assert_eq!(wine_folder(OsString::new()), None);
    let path = join(OsStr::new("Z:/Users/me/dump"), "f9-1\\stats.txt");
    assert_eq!(path, "Z:/Users/me/dump\\f9-1\\stats.txt");
    assert_eq!(
        platform::host_bytes(path.as_encoded_bytes()),
        b"/Users/me/dump/f9-1/stats.txt"
    );
    assert_eq!(platform::host_bytes(b"z:\\dump\\a"), b"/dump/a");
    assert_eq!(platform::host_bytes(b"Z:dump"), b"Z:dump");
    assert_eq!(platform::host_bytes(b"rel\\dump"), b"rel/dump");
}
