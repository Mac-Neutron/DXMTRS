use std::{
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use super::{KEEP, OpenOutcome, PENDING_CAP, PeLog, TRUNCATED_NOTE, WriteOutcome};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mtld3d-pe-log-test-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn pending_caps_at_one_mib_with_single_truncation_note() {
    let dir = TempDir::new("cap");
    let mut log = PeLog::new();
    let half = vec![b'a'; PENDING_CAP / 2];
    assert_eq!(log.write_line(&half), WriteOutcome::Ok);
    assert_eq!(log.write_line(&half), WriteOutcome::Ok);
    assert_eq!(log.write_line(b"dropped-1\n"), WriteOutcome::Ok);
    assert_eq!(log.write_line(b"dropped-2\n"), WriteOutcome::Ok);

    let OpenOutcome::Opened(path) = log.open(dir.path().to_path_buf(), "game".into(), 42) else {
        panic!("expected Opened");
    };
    let contents = fs::read(&path).expect("read opened log");
    assert_eq!(contents.len(), PENDING_CAP + TRUNCATED_NOTE.len());
    assert!(contents.ends_with(TRUNCATED_NOTE));
}

#[test]
fn open_is_idempotent_and_lazy_until_first_non_empty_line() {
    let dir = TempDir::new("lazy");
    let log_dir = dir.path().join("nested/logs");
    let mut log = PeLog::new();

    assert_eq!(
        log.open(log_dir.clone(), "game".into(), 100),
        OpenOutcome::Deferred
    );
    assert!(!log_dir.exists(), "empty pending must not create dir/file");
    assert_eq!(
        log.open(log_dir.clone(), "other".into(), 999),
        OpenOutcome::AlreadyOpen
    );
    assert_eq!(log.write_line(b""), WriteOutcome::Ok);
    assert!(!log_dir.exists(), "empty write keeps log lazy");

    let WriteOutcome::Opened(path) = log.write_line(b"first warning\n") else {
        panic!("expected Lazy to open on first non-empty write");
    };
    assert_eq!(path, log_dir.join("game-100.log"));
    assert_eq!(
        log.open(log_dir, "ignored".into(), 200),
        OpenOutcome::AlreadyOpen
    );
    assert_eq!(log.write_line(b"second line\n"), WriteOutcome::Ok);
    drop(log);

    assert_eq!(
        fs::read_to_string(path).expect("read log"),
        "first warning\nsecond line\n"
    );
}

#[test]
fn collision_suffixes_increment_without_clobbering_existing_logs() {
    let dir = TempDir::new("collision");
    fs::write(dir.path().join("app-77.log"), b"existing-1\n").unwrap();
    fs::write(dir.path().join("app-77-2.log"), b"existing-2\n").unwrap();

    let mut log = PeLog::new();
    assert_eq!(log.write_line(b"new-run\n"), WriteOutcome::Ok);
    let OpenOutcome::Opened(path) = log.open(dir.path().to_path_buf(), "app".into(), 77) else {
        panic!("expected Opened");
    };
    assert_eq!(path, dir.path().join("app-77-3.log"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "new-run\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("app-77.log")).unwrap(),
        "existing-1\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("app-77-2.log")).unwrap(),
        "existing-2\n"
    );
}

#[test]
fn prune_keeps_nine_oldest_trimmed_regular_logs_and_ignores_directories() {
    let dir = TempDir::new("prune");
    let fake_dir = dir.path().join("decoy.log");
    fs::create_dir(&fake_dir).unwrap();
    let keep_non_log = dir.path().join("notes.txt");
    fs::write(&keep_non_log, b"keep").unwrap();

    let base_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    for idx in 0..12_u64 {
        let p = dir.path().join(format!("old-{idx:02}.log"));
        let mut f = File::create(&p).unwrap();
        writeln!(f, "entry {idx}").unwrap();
        f.set_modified(base_time + Duration::from_secs(idx * 10))
            .unwrap();
    }

    let mut log = PeLog::new();
    assert_eq!(log.write_line(b"latest\n"), WriteOutcome::Ok);
    let OpenOutcome::Opened(created) = log.open(dir.path().to_path_buf(), "fresh".into(), 5) else {
        panic!("expected Opened");
    };
    assert!(fake_dir.is_dir(), "directory ending in .log must stay");
    assert!(keep_non_log.is_file(), "non-log file must stay");
    assert!(created.is_file());

    let remaining_logs: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .filter_map(|e| {
            let p = e.path();
            (p.extension().is_some_and(|x| x == "log"))
                .then(|| p.file_name().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    assert_eq!(remaining_logs.len(), KEEP);
    for evicted in ["old-00.log", "old-01.log", "old-02.log"] {
        assert!(!dir.path().join(evicted).exists(), "{evicted} should prune");
    }
    for kept in 3..12 {
        assert!(dir.path().join(format!("old-{kept:02}.log")).exists());
    }
}

#[test]
fn open_error_falls_back_to_stderr_state() {
    let dir = TempDir::new("err");
    let blocker = dir.path().join("not-a-dir");
    fs::write(&blocker, b"file").unwrap();

    let mut log = PeLog::new();
    assert_eq!(log.write_line(b"line\n"), WriteOutcome::Ok);
    let outcome = log.open(blocker, "app".into(), 9);
    assert!(matches!(outcome, OpenOutcome::Stderr(_)));
    assert!(log.file().is_none());
    assert_eq!(log.write_line(b"after\n"), WriteOutcome::Ok);
}
