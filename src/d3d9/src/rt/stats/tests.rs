use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use winemetal_sys::Obj;

use crate::rt::{
    device::{CoreDevice, fixture::FAKE_GLOBAL_LOCK, queue_profile_for},
    flush_stats_on_detach,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("d3d9-rt-stats-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
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
fn dxmt_stats_emits_stats_txt_on_detach() {
    if std::env::var_os("D3D9_RT_STATS_CHILD").is_some() {
        let _guard = FAKE_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: synthetic device handle used only by `dxmt-fakes`.
        let mtl = unsafe { Obj::adopt(0xd9) }.expect("non-zero handle");
        let mut core = CoreDevice::new(mtl, queue_profile_for(32));
        core.present(false, 0.0, 0.0, false);
        core.present(false, 0.0, 0.0, false);
        drop(core);
        flush_stats_on_detach();
        return;
    }

    let dir = TempDir::new("emit");
    let exe = std::env::current_exe().expect("current test binary");
    let status = Command::new(exe)
        .arg("--exact")
        .arg("rt::stats::tests::dxmt_stats_emits_stats_txt_on_detach")
        .env("D3D9_RT_STATS_CHILD", "1")
        .env("DXMT_STATS", "1")
        .env("DXMT_DXIL_DUMP", dir.path())
        .status()
        .expect("spawn child test");
    assert!(status.success(), "child stats process failed: {status}");

    let stats_path = dir.path().join("stats.txt");
    assert!(
        stats_path.is_file(),
        "expected stats.txt at {}",
        stats_path.display()
    );
    let report = fs::read_to_string(&stats_path).expect("read stats.txt");
    assert!(report.contains("exit"), "expected exit report: {report}");
    assert!(
        report.contains("2 frames ("),
        "expected 2-frame count in stats report: {report}"
    );
    assert!(
        report.contains("counters:"),
        "expected counters section in stats report: {report}"
    );
}
