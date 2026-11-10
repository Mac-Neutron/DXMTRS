//! Tests for the cache-writing modes.
//!
//! The synthetic cache loads with every record, and a merge keeps what its inputs held that
//! has a source.

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
    // The timed set plus what only the cache file carries: the `ps_1_x` texture-address families, the modifier
    // programs, the `oDepth` writers, the vertex shader that feeds the depth families and the
    // `ps_3_0` reading the passthrough extras.
    assert_eq!(
        records
            .shaders
            .iter()
            .filter(|e| e.kind.is_programmable())
            .count(),
        corpus.shaders.len()
            + texture_family_ps().len()
            + modifier_programs().len()
            + depth_programs().len()
            + 2
    );
    assert!(records.shaders.iter().all(|e| e.source().is_some()));
    assert!(!records.pipelines.is_empty());
    // Every fixed-function key of both lists has its record, built by this tree's `ff_build`.
    let has = |entry: &CacheEntry| records.shaders.iter().any(|e| e == entry);
    for shader in synthetic_ff().into_iter().chain(synthetic_ff_extra()) {
        let entry = match &shader.input {
            Input::FfVs(key) => ff_vs_record(key),
            Input::FfPs(key, variant) => ff_ps_record(key, *variant),
            Input::Synthetic { .. } | Input::Record(_) => continue,
        }
        .expect("a fixed-function record")
        .1;
        assert!(has(&entry), "{:?} {:016x}", entry.kind, entry.key);
    }
    assert!(
        records
            .shaders
            .iter()
            .filter_map(CacheEntry::source)
            .all(|source| source.rebuilt_ff().is_none())
    );
    // Every MSL kind is present, and every recipe pairs a vertex with a pixel record.
    for kind in [
        CachedKind::Sm1Vs,
        CachedKind::Sm1Ps,
        CachedKind::Sm2Vs,
        CachedKind::Sm2Ps,
        CachedKind::Sm3Vs,
        CachedKind::Sm3Ps,
        CachedKind::FfVs,
        CachedKind::FfPs,
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

#[test]
fn kinds_counts_msl_and_air_records() {
    let path = temp("kinds-src");
    write_synthetic_cache(&path, &synthetic()).expect("write");
    let records = loaded(&path).shaders;
    let vs = records
        .iter()
        .find(|e| e.kind == CachedKind::Sm3Vs)
        .expect("sm3 vs");
    let ps = records
        .iter()
        .find(|e| e.kind == CachedKind::Sm3Ps)
        .expect("sm3 ps");
    let air = CacheEntry::new_air(
        CachedKind::AirSm3Ps,
        ps.key,
        [7; 32],
        vec![1, 2, 3],
        ps.source().expect("source").clone(),
    );
    let out = temp("kinds");
    write_cache_file(&out, &[vs.clone(), ps.clone(), air], &[]).expect("write kinds fixture");
    let line = kinds_line(&[out.clone(), out.clone()]).expect("kinds");
    assert_eq!(
        line,
        "kinds 2 files: sm1vs=0 sm1ps=0 sm2vs=0 sm2ps=0 sm3vs=2 sm3ps=2 air_sm1vs=0 air_sm1ps=0 \
         air_sm2vs=0 air_sm2ps=0 air_sm3vs=0 air_sm3ps=2 ffvs=0 ffps=0 air_ffvs=0 air_ffps=0"
    );
    assert!(kinds_line(&[temp("kinds-missing")]).is_err());
    // An AIR record is not MSL to time.
    assert!(Shader::record(loaded(&out).shaders.pop().expect("air")).is_none());
}

/// A programmable pair from the synthetic cache: a `vs_3_0` and a `ps_3_0` record.
///
/// Each test names its own file: tests run in parallel in one process.
fn sm3_pair(name: &str) -> (CacheEntry, CacheEntry) {
    let path = temp(name);
    write_synthetic_cache(&path, &synthetic()).expect("write");
    let records = loaded(&path).shaders;
    let find = |kind| {
        records
            .iter()
            .find(|e| e.kind == kind && e.source().is_some())
            .cloned()
            .expect("a programmable record")
    };
    (find(CachedKind::Sm3Vs), find(CachedKind::Sm3Ps))
}

fn recipe(vs: &CacheEntry, ps: &CacheEntry) -> PipelineRecipe {
    synthetic_recipe(
        ShaderRecordRef::new(vs.kind, vs.key),
        ShaderRecordRef::new(ps.kind, ps.key),
        16,
        &[],
        PixelFormat::Bgra8Unorm,
        false,
    )
}

#[test]
fn merge_drops_sourceless_records_and_their_recipes() {
    let (vs, ps) = sm3_pair("sourceless-pair");
    let key = carrier_vs();
    let (_, with_source) = ff_vs_record(&key).expect("ff vs");
    // The MSL fixed-function record a C3b runtime writes: no source.
    let sourceless = CacheEntry::new(
        CachedKind::FfVs,
        with_source.key,
        with_source.msl().expect("msl").to_owned(),
        None,
    );
    let path = temp("sourceless-in");
    write_cache_file(
        &path,
        &[vs.clone(), ps.clone(), sourceless.clone()],
        &[recipe(&vs, &ps), recipe(&sourceless, &ps)],
    )
    .expect("write");
    assert_eq!(
        loaded(&path).shaders.len(),
        3,
        "the input keeps the sourceless record"
    );
    let out = temp("sourceless-out");
    let line = merge_caches(&out, &[path]).expect("merge");
    assert!(
        line.starts_with("merge: dropped 1 sourceless records, 1 recipes\n"),
        "{line}"
    );
    let merged = loaded(&out);
    assert_eq!(merged.shaders, vec![vs.clone(), ps]);
    assert_eq!(merged.pipelines.len(), 1);
    assert_eq!(merged.pipelines[0].vs().key(), vs.key);
}

#[test]
fn merge_keeps_air_records_with_sources() {
    let (vs, ps) = sm3_pair("air-pair");
    let (_, vertex_ff) = ff_vs_record(&carrier_vs()).expect("ff vs");
    let (ps_key, variant) = carrier_ps(&carrier_vs());
    let (_, pixel_ff) = ff_ps_record(&ps_key, variant).expect("ff ps");
    let air = |kind, entry: &CacheEntry| {
        CacheEntry::new_air(
            kind,
            entry.key,
            [7; 32],
            vec![1, 2, 3],
            entry.source().expect("source").clone(),
        )
    };
    let records = [
        air(CachedKind::AirFfVs, &vertex_ff),
        air(CachedKind::AirFfPs, &pixel_ff),
        air(CachedKind::AirSm3Vs, &vs),
        air(CachedKind::AirSm3Ps, &ps),
        vertex_ff,
        pixel_ff,
    ];
    let pipelines = [
        recipe(&records[0], &records[1]),
        recipe(&records[2], &records[3]),
        recipe(&records[4], &records[5]),
    ];
    let path = temp("air-in");
    write_cache_file(&path, &records, &pipelines).expect("write");
    let out = temp("air-out");
    let line = merge_caches(&out, &[path]).expect("merge");
    assert!(
        line.starts_with("merge: dropped 0 sourceless records, 0 recipes\n"),
        "{line}"
    );
    let merged = loaded(&out);
    assert_eq!(merged.shaders, records.to_vec());
    assert_eq!(merged.pipelines.len(), pipelines.len());
}
