//! Tests for the cache-writing modes.
//!
//! The synthetic cache loads with every record, and a merge keeps what its inputs held.

use mtld3d_core::shader_cache::{CacheLoad, load};

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("emit-corpus-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    dir.join("cache.bin")
}

fn synthetic() -> Corpus {
    Corpus::new("synthetic_sm".to_owned(), synthetic_sm())
}

fn loaded(path: &Path) -> mtld3d_core::shader_cache::CacheRecords {
    match load(path).expect("load") {
        CacheLoad::Current(records) => records,
        _ => panic!("{} is not a current cache", path.display()),
    }
}

#[test]
fn written_cache_loads_with_every_record() {
    let corpus = synthetic();
    assert_eq!(corpus.failures, 0);
    let path = temp("written");
    write_synthetic_cache(&path, &corpus).expect("write");
    let records = loaded(&path);
    assert_eq!(records.shaders.len(), corpus.shaders.len());
    assert!(records.shaders.iter().all(|e| e.source().is_some()));
    assert!(!records.pipelines.is_empty());
    // Every programmable kind is present, and every recipe pairs a vertex with a pixel record.
    for kind in [
        CachedKind::Sm1Vs,
        CachedKind::Sm1Ps,
        CachedKind::Sm2Vs,
        CachedKind::Sm2Ps,
        CachedKind::Sm3Vs,
        CachedKind::Sm3Ps,
    ] {
        assert!(records.shaders.iter().any(|e| e.kind == kind), "{kind:?}");
    }
    for recipe in &records.pipelines {
        assert!(
            records
                .shaders
                .iter()
                .any(|e| (e.kind, e.key) == (recipe.vs().kind(), recipe.vs().key()))
        );
        assert!(
            records
                .shaders
                .iter()
                .any(|e| (e.kind, e.key) == (recipe.ps().kind(), recipe.ps().key()))
        );
    }
}

#[test]
fn merge_of_one_file_keeps_every_record() {
    let path = temp("merge-in");
    write_synthetic_cache(&path, &synthetic()).expect("write");
    let out = temp("merge-out");
    merge_caches(&out, std::slice::from_ref(&path)).expect("merge");
    let (before, after) = (loaded(&path), loaded(&out));
    assert_eq!(before.shaders, after.shaders);
    assert_eq!(before.pipelines.len(), after.pipelines.len());
    // The same file twice adds nothing.
    merge_caches(&out, &[path.clone(), path]).expect("merge twice");
    let again = loaded(&out);
    assert_eq!(again.shaders.len(), before.shaders.len());
    assert_eq!(again.pipelines.len(), before.pipelines.len());
}

#[test]
fn merge_keeps_the_valid_prefix_of_a_torn_file() {
    let path = temp("torn-in");
    write_synthetic_cache(&path, &synthetic()).expect("write");
    let before = loaded(&path);
    let mut bytes = fs::read(&path).expect("read");
    bytes.extend_from_slice(&[0xA5; 37]);
    fs::write(&path, bytes).expect("append junk");
    let out = temp("torn-out");
    merge_caches(&out, &[path]).expect("merge");
    let after = loaded(&out);
    assert_eq!(after.shaders, before.shaders);
    assert_eq!(after.pipelines.len(), before.pipelines.len());
}
