//! Unit tests for the on-disk shader-cache binary format.
//!
//! Hand-built files cover the round trips, the damage paths (torn tail, flipped chunk-header
//! bit, scrambled zstd frame, unknown chunk kind, stray file header), duplicate keys and a
//! header-only file. Every one of those but the scrambled-frame case pins the
//! `needs_compaction` verdict the pre-warm rewrite keys off. The file-level tests write real
//! files under the system temp root and pin that many writers reaching a cold cache together
//! still produce one header. Further tests cover header validation.

use super::*;
use crate::shader_key::vs_source_disk_key_programmable;

fn set_emitter(entry: &mut CacheEntry, value: u64) {
    if let EntryBody::Msl { emitter, .. } = &mut entry.body {
        *emitter = value;
    }
}

fn flip_emitter(entry: &mut CacheEntry) {
    if let EntryBody::Msl { emitter, .. } = &entry.body {
        let flipped = *emitter ^ 1;
        set_emitter(entry, flipped);
    }
}

fn write_file(entries_per_chunk: &[Vec<CacheEntry>], bundle_last: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    write_header(&mut buf);
    let last_idx = entries_per_chunk.len().saturating_sub(1);
    for (i, group) in entries_per_chunk.iter().enumerate() {
        let as_bundle = bundle_last && i == last_idx;
        if as_bundle {
            write_bundle(&mut buf, group, &[]);
        } else {
            for entry in group {
                write_record(&mut buf, entry);
            }
        }
    }
    buf
}

/// Count the occurrences of the file magic in `bytes`.
///
/// A file carrying its header once has exactly one; a duplicate creation
/// leaves a second run of the magic further in.
fn magic_count(bytes: &[u8]) -> usize {
    bytes
        .windows(SHADER_CACHE_MAGIC.len())
        .filter(|w| **w == SHADER_CACHE_MAGIC)
        .count()
}

/// An empty directory under the system temp root, unique to this process and this call.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "mtld3d-shader-cache-{tag}-{}-{seq}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Append one record to the cache file at `path`, creating the file when absent.
fn append_one(path: &std::path::Path, entry: &CacheEntry) {
    let writer = CacheWriter::open(path).expect("open cache writer");
    writer.append_shader(entry).expect("append record");
}

fn read_shaders(bytes: &[u8]) -> (Vec<CacheEntry>, bool) {
    let records = read_records(bytes);
    assert!(records.pipelines.is_empty());
    (records.shaders, records.needs_compaction)
}

fn sample_entries() -> Vec<CacheEntry> {
    vec![
        CacheEntry {
            source: None,
            kind: CachedKind::Sm3Vs,
            key: 0xDEAD_BEEF_CAFE_BABE,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "vertex VsOut vs(Inputs in [[stage_in]]) { /* … */ }".into(),
            },
        },
        CacheEntry {
            source: None,
            kind: CachedKind::FfPs,
            key: 0,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: String::new(),
            },
        },
        CacheEntry {
            source: None,
            kind: CachedKind::Sm2Ps,
            key: 0x0102_0304_0506_0708,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "fragment float4 ps() { return float4(1); }".into(),
            },
        },
    ]
}

fn sample_recipe() -> PipelineRecipe {
    let entries = sample_entries();
    let mut stream_layouts = [StreamLayout::UNUSED; MAX_STREAMS as usize];
    stream_layouts[0] = StreamLayout {
        stride: 12,
        step: VertexStepFunction::PerVertex,
        step_rate: 1,
    };
    let snapshot = PipelineSnapshot {
        vs_fn: MetalHandle::NULL,
        ps_fn: MetalHandle::NULL,
        vdecl_hash: 0x1234,
        stream_layouts,
        color_format: PixelFormat::Bgra8Unorm,
        attach: PipelineAttachFlags::HAS_COLOR_OUTPUT | PipelineAttachFlags::COLOR_HAS_ALPHA,
        rs: PipelineRsBits {
            color_write_mask: 0x0F,
            ..PipelineRsBits::default()
        },
        extra: ExtraColorAttachments::NONE,
        ps_color_out_mask: 1,
        sample_count: 1,
    };
    PipelineRecipe::from_snapshot(
        ShaderRecordRef::new(entries[0].kind, entries[0].key),
        ShaderRecordRef::new(entries[2].kind, entries[2].key),
        &snapshot,
        &[VertexAttrDesc {
            attr_index: 0,
            buffer_index: 0,
            offset: 0,
            format: VertexFormat::Float3,
        }],
    )
}

#[test]
fn single_chunk_round_trip() {
    let entries = sample_entries();
    let buf = write_file(std::slice::from_ref(&entries), false);
    assert_eq!(read_header(&buf), Ok(CacheHeader::CURRENT));
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read, entries);
    // Singles only, no Bundle ⇒ compact next launch.
    assert!(needs_compaction);
}

#[test]
fn bundle_chunk_round_trip_is_optimal() {
    let entries = sample_entries();
    let buf = write_file(std::slice::from_ref(&entries), true);
    assert_eq!(read_header(&buf), Ok(CacheHeader::CURRENT));
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read, entries);
    // Exactly one Bundle, no dupes, EOF clean ⇒ optimal.
    assert!(!needs_compaction);
    // First chunk byte after the file header is the Bundle discriminator.
    assert_eq!(buf[HEADER_LEN], RECORD_KIND_BUNDLE);
}

#[test]
fn pipeline_recipe_round_trips_with_stable_shader_refs() {
    let entries = sample_entries();
    let recipe = sample_recipe();
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_bundle(&mut buf, &entries, std::slice::from_ref(&recipe));
    let records = read_records(&buf);
    assert_eq!(records.shaders, entries);
    assert!(records.pipelines == vec![recipe]);
    assert!(!records.needs_compaction);
}

#[test]
fn coverage_pipeline_recipe_preserves_enable_state() {
    let mut recipe = sample_recipe();
    recipe.snapshot.sample_count = 4;
    recipe
        .snapshot
        .rs
        .flags
        .insert(PipelineRsFlags::ALPHA_TO_COVERAGE);
    let mut bytes = Vec::new();
    recipe.encode(&mut bytes);
    let decoded = PipelineRecipe::decode(&bytes).expect("coverage recipe");
    assert!(
        decoded
            .snapshot
            .rs
            .alpha_to_coverage(decoded.snapshot.sample_count)
    );
    assert!(decoded == recipe);
}

#[test]
fn attachmentless_recipes_are_removed_without_losing_valid_records() {
    let entries = sample_entries();
    let mut empty = sample_recipe();
    empty.snapshot.attach = PipelineAttachFlags::empty();
    empty.snapshot.rs.color_write_mask = 0;
    let mut depth_only = sample_recipe();
    depth_only.snapshot.attach = PipelineAttachFlags::HAS_DEPTH;
    depth_only.snapshot.rs.color_write_mask = 0;
    let mut extra_only = sample_recipe();
    extra_only.snapshot.attach = PipelineAttachFlags::empty();
    extra_only.snapshot.extra.present_mask = 1;
    let recipes = [sample_recipe(), empty, depth_only, extra_only];

    for bundled in [false, true] {
        let mut bytes = Vec::new();
        write_header(&mut bytes);
        if bundled {
            write_bundle(&mut bytes, &entries, &recipes);
        } else {
            for entry in &entries {
                write_record(&mut bytes, entry);
            }
            for recipe in &recipes {
                write_pipeline_record(&mut bytes, recipe);
            }
        }
        let records = read_records(&bytes);
        assert_eq!(records.shaders, entries);
        assert_eq!(records.pipelines.len(), 3);
        assert!(records.pipelines[0] == recipes[0]);
        assert!(records.pipelines[1] == recipes[2]);
        assert!(records.pipelines[2] == recipes[3]);
        assert!(records.needs_compaction);

        let mut compacted = Vec::new();
        write_header(&mut compacted);
        write_bundle(&mut compacted, &records.shaders, &records.pipelines);
        let reloaded = read_records(&compacted);
        assert_eq!(reloaded.shaders, entries);
        assert!(reloaded.pipelines == records.pipelines);
        assert!(!reloaded.needs_compaction);
    }
}

#[test]
fn pipeline_recipe_without_both_shader_records_is_dropped() {
    let entries = sample_entries();
    let recipe = sample_recipe();
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_bundle(&mut buf, &entries[..1], std::slice::from_ref(&recipe));
    let records = read_records(&buf);
    assert!(records.pipelines.is_empty());
    assert!(records.needs_compaction);
}

#[test]
fn load_repairs_a_torn_tail_before_another_writer_appends() {
    let dir = scratch_dir("load-repair");
    let path = dir.join("mtld3d_shaders.bin");
    let entries = sample_entries();
    let mut bytes = write_file(std::slice::from_ref(&entries), false);
    bytes.truncate(bytes.len() - 5);
    std::fs::write(&path, bytes).expect("write torn cache");

    let CacheLoad::Current(records) = load(&path).expect("load torn cache") else {
        panic!("torn cache lost its valid header");
    };
    assert_eq!(records.shaders, entries[..2]);
    // A live encoder can write while another device compiles its startup
    // snapshot. Its records must remain reachable by the later compactor.
    append_one(&path, &entries[2]);
    compact(&path).expect("compact repaired cache");
    let records = read_records(&std::fs::read(&path).expect("read repaired cache"));
    assert_eq!(records.shaders, entries);
    assert!(!records.needs_compaction);
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn recipes_preserve_instancing_constant_streams_mrt_and_siblings() {
    let mut recipe = sample_recipe();
    recipe.snapshot.stream_layouts[0].step = VertexStepFunction::Constant;
    recipe.snapshot.stream_layouts[0].step_rate = 0;
    recipe.snapshot.stream_layouts[15] = StreamLayout {
        stride: 32,
        step: VertexStepFunction::PerInstance,
        step_rate: 7,
    };
    recipe.vertex_attrs.push(VertexAttrDesc {
        attr_index: 15,
        buffer_index: 15,
        offset: 16,
        format: VertexFormat::Float4,
    });
    recipe.snapshot.sample_count = 4;
    recipe
        .snapshot
        .attach
        .insert(PipelineAttachFlags::HAS_DEPTH | PipelineAttachFlags::HAS_STENCIL);
    recipe.snapshot.extra = ExtraColorAttachments {
        formats: [
            PixelFormat::Rgba16Float,
            PixelFormat::Bgra8Unorm,
            PixelFormat::R32Float,
        ],
        present_mask: 0b101,
        has_alpha_mask: 0b001,
    };
    recipe.snapshot.ps_color_out_mask = 0b1011;
    recipe.snapshot.rs.flags = PipelineRsFlags::all();
    recipe.snapshot.rs.src_blend_alpha = 5;
    recipe.snapshot.rs.dst_blend_alpha = 6;
    recipe.snapshot.rs.blend_op_alpha = 3;
    recipe.snapshot.rs.color_write_mask = 0;
    recipe.snapshot.rs.color_write_mask_ext = [0; 3];
    let mut sibling_snapshot = recipe.snapshot.clone();
    sibling_snapshot
        .attach
        .remove(PipelineAttachFlags::HAS_COLOR_OUTPUT);
    sibling_snapshot.extra = ExtraColorAttachments::NONE;
    let sibling = PipelineRecipe::from_snapshot(
        recipe.vs,
        recipe.ps,
        &sibling_snapshot,
        recipe.vertex_attrs(),
    );

    let mut bytes = Vec::new();
    write_header(&mut bytes);
    let recipes = [recipe, sibling];
    write_bundle(&mut bytes, &sample_entries(), &recipes);
    let records = read_records(&bytes);
    assert!(records.pipelines == recipes);
    assert!(!records.needs_compaction);
}

#[test]
fn resolved_recipe_keys_like_the_live_draw_it_recorded() {
    let attrs = [VertexAttrDesc {
        attr_index: 0,
        buffer_index: 0,
        offset: 0,
        format: VertexFormat::Float3,
    }];
    let mut live = sample_recipe().resolve(MetalHandle::NULL, MetalHandle::NULL);
    // SAFETY: tests; opaque values never dereferenced.
    live.vs_fn = unsafe { MetalHandle::new(0x10) };
    // SAFETY: tests; opaque values never dereferenced.
    live.ps_fn = unsafe { MetalHandle::new(0x20) };
    let entries = sample_entries();
    let recipe = PipelineRecipe::from_snapshot(
        ShaderRecordRef::new(entries[0].kind, entries[0].key),
        ShaderRecordRef::new(entries[2].kind, entries[2].key),
        &live,
        &attrs,
    );
    let mut bytes = Vec::new();
    recipe.encode(&mut bytes);
    let decoded = PipelineRecipe::decode(&bytes).expect("decode recipe");
    assert_eq!(
        crate::pipeline_state::key_from_snapshot(
            &decoded.resolve(live.vs_fn, live.ps_fn),
            decoded.vertex_attrs()
        ),
        crate::pipeline_state::key_from_snapshot(&live, &attrs)
    );
}

#[test]
fn duplicate_recipes_share_shader_records_after_compaction() {
    let dir = scratch_dir("recipe-dedup");
    let path = dir.join("mtld3d_shaders.bin");
    let entries = sample_entries();
    let writer = CacheWriter::open(&path).expect("open writer");
    for entry in &entries {
        writer.append_shader(entry).expect("append shader");
        writer
            .append_shader(entry)
            .expect("append duplicate shader");
    }
    let first = sample_recipe();
    let mut second = sample_recipe();
    second
        .snapshot
        .rs
        .flags
        .insert(PipelineRsFlags::BLEND_ENABLE);
    for recipe in [&first, &second, &first] {
        writer.append_pipeline(recipe).expect("append recipe");
    }
    compact(&path).expect("compact recipes");
    let records = read_records(&std::fs::read(&path).expect("read compacted recipes"));
    assert_eq!(records.shaders, entries);
    assert!(records.pipelines == [first, second]);
    assert!(!records.needs_compaction);
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn malformed_pipeline_payloads_are_rejected_with_valid_checksums() {
    let mut valid = Vec::new();
    sample_recipe().encode(&mut valid);
    for length in 0..valid.len() {
        assert!(PipelineRecipe::decode(&valid[..length]).is_none());
    }
    for (offset, value) in [(0, CachedKind::FfPs as u8), (2, 0x80), (30, 0), (31, 0x80)] {
        let mut invalid = valid.clone();
        invalid[offset] = value;
        let frame = zstd::encode_all(invalid.as_slice(), ZSTD_APPEND_LEVEL)
            .expect("compress invalid recipe");
        let mut bytes = write_file(&[sample_entries()], false);
        push_chunk(
            &mut bytes,
            RECORD_KIND_PIPELINE,
            sample_recipe().disk_key(),
            &frame,
        );
        let records = read_records(&bytes);
        assert!(records.pipelines.is_empty());
        assert!(records.needs_compaction);
    }
}

#[test]
fn mixed_bundle_plus_singles_round_trip() {
    let bundle_entries = sample_entries();
    let later_appends = vec![CacheEntry {
        source: None,
        kind: CachedKind::Sm3Ps,
        key: 0xAAAA_BBBB_CCCC_DDDD,
        body: EntryBody::Msl {
            emitter: SHADER_EMITTER_VERSION,
            msl: "fragment float4 ps_later() { return float4(0,1,0,1); }".into(),
        },
    }];
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_bundle(&mut buf, &bundle_entries, &[]);
    for e in &later_appends {
        write_record(&mut buf, e);
    }
    let (read, needs_compaction) = read_shaders(&buf);
    let mut expected = bundle_entries.clone();
    expected.extend(later_appends);
    assert_eq!(read, expected);
    // Bundle + singles ⇒ not optimal.
    assert!(needs_compaction);
}

#[test]
fn torn_trailing_chunk_dropped_and_flags_compaction() {
    let entries = sample_entries();
    let mut buf = write_file(std::slice::from_ref(&entries), false);
    assert_eq!(read_records(&buf).valid_len(), buf.len(), "an intact file");
    // Truncate mid-frame of the final chunk.
    buf.truncate(buf.len() - 5);
    let (read, needs_compaction) = read_shaders(&buf);
    // Dropped the torn last chunk.
    assert_eq!(read.len(), entries.len() - 1);
    assert!(needs_compaction);
    assert!(
        read_records(&buf).valid_len() < buf.len(),
        "the valid prefix ends before the torn chunk"
    );
}

#[test]
fn corrupt_chunk_header_caught_by_xxh3() {
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm2Vs,
            key: 0xAABB,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "ok before".into(),
            },
        },
    );
    let after_first = buf.len();
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm2Ps,
            key: 0xCCDD,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "corrupted in header below".into(),
            },
        },
    );
    // Flip a bit in the second chunk's `frame_len` field. Without
    // the xxh3 this would silently desync every subsequent parse;
    // with it, the chunk is detected as corrupt and we stop here.
    buf[after_first + 12] ^= 0x01;
    // Append one more well-formed chunk; since we can't trust the
    // corrupt frame_len to skip safely, this trailing chunk is
    // intentionally forfeit (recompiled next session).
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm3Ps,
            key: 0xEEFF,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "ok after — forfeit on corruption-stop".into(),
            },
        },
    );
    let (read, needs_compaction) = read_shaders(&buf);
    // Only the chunk before the corruption survives. The corrupt
    // chunk and everything after it are dropped; compaction rewrites
    // a clean file so the trailing chunk recompiles next launch.
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].key, 0xAABB);
    assert!(needs_compaction);
}

#[test]
fn corrupt_frame_body_caught_and_skipped() {
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm2Vs,
            key: 0x1111,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "good".into(),
            },
        },
    );
    let bad_start = buf.len();
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm2Ps,
            key: 0x2222,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "frame body will be scrambled".into(),
            },
        },
    );
    // Scramble a byte inside the second chunk's compressed frame
    // (past the 24-byte chunk header).
    buf[bad_start + CHUNK_HEADER_LEN + 2] ^= 0xFF;
    let (read, _) = read_shaders(&buf);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].key, 0x1111);
}

#[test]
fn unknown_chunk_kind_skipped_via_frame_len() {
    let mut buf = Vec::new();
    write_header(&mut buf);
    // Hand-craft a chunk with kind = 0x42 (neither CachedKind nor Bundle),
    // valid xxh3, a tiny zstd frame as payload. Use write_record to
    // build a Single, then patch its kind byte after the fact and
    // recompute the checksum so the parser reaches the unknown-kind
    // arm rather than failing on xxh3.
    let weird_kind: u8 = 0x42;
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::FfVs,
            key: 0x9999,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "irrelevant".into(),
            },
        },
    );
    let chunk_off = HEADER_LEN;
    buf[chunk_off] = weird_kind;
    let frame_len =
        u32::from_le_bytes(buf[chunk_off + 12..chunk_off + 16].try_into().unwrap()) as usize;
    let header16: [u8; 16] = buf[chunk_off..chunk_off + 16].try_into().unwrap();
    let frame_start = chunk_off + CHUNK_HEADER_LEN;
    let frame = &buf[frame_start..frame_start + frame_len];
    let new_checksum = chunk_xxh3(&header16, frame);
    buf[chunk_off + 16..chunk_off + 24].copy_from_slice(&new_checksum.to_le_bytes());
    // Followed by a valid chunk.
    write_record(
        &mut buf,
        &CacheEntry {
            source: None,
            kind: CachedKind::Sm3Ps,
            key: 0x4321,
            body: EntryBody::Msl {
                emitter: SHADER_EMITTER_VERSION,
                msl: "after weird".into(),
            },
        },
    );
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].key, 0x4321);
    assert!(needs_compaction);
}

#[test]
fn duplicate_keys_flag_compaction() {
    let dup = CacheEntry {
        source: None,
        kind: CachedKind::Sm3Vs,
        key: 0x5555,
        body: EntryBody::Msl {
            emitter: SHADER_EMITTER_VERSION,
            msl: "first copy".into(),
        },
    };
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_bundle(&mut buf, &[dup.clone(), dup], &[]);
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read.len(), 1);
    assert!(needs_compaction);
}

#[test]
fn empty_file_with_just_header_is_compacted() {
    let mut buf = Vec::new();
    write_header(&mut buf);
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read, [] as [CacheEntry; 0]);
    assert!(needs_compaction);
}

#[test]
fn read_header_rejects_wrong_magic() {
    let bytes = b"GARBAGE!\x01\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(read_header(bytes), Err(CacheReadError::WrongMagic));
}

#[test]
fn read_header_rejects_short_input() {
    let bytes = b"MTLD3DSH";
    assert_eq!(read_header(bytes), Err(CacheReadError::WrongMagic));
}

#[test]
fn read_header_returns_both_versions() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&SHADER_CACHE_MAGIC);
    buf.extend_from_slice(&99u32.to_le_bytes());
    buf.extend_from_slice(&100u32.to_le_bytes());
    assert_eq!(
        read_header(&buf),
        Ok(CacheHeader {
            format_version: 99,
            shader_schema_version: 100,
        })
    );
}

#[test]
fn stray_file_header_mid_file_is_skipped() {
    let entries = sample_entries();
    let mut buf = Vec::new();
    write_header(&mut buf);
    write_record(&mut buf, &entries[0]);
    // A second creation's header, landing where a chunk header belongs.
    write_header(&mut buf);
    write_record(&mut buf, &entries[1]);
    let (read, needs_compaction) = read_shaders(&buf);
    assert_eq!(read, vec![entries[0].clone(), entries[1].clone()]);
    // The stray header is a reason to rewrite the file dense.
    assert!(needs_compaction);
}

#[test]
fn open_for_append_writes_one_header_across_writers() {
    let dir = scratch_dir("append");
    let path = dir.join("mtld3d_shaders.bin");
    let entries = sample_entries();
    for entry in &entries {
        append_one(&path, entry);
    }
    let bytes = std::fs::read(&path).expect("read cache file");
    assert_eq!(magic_count(&bytes), 1);
    assert_eq!(read_header(&bytes), Ok(CacheHeader::CURRENT));
    let (read, _) = read_shaders(&bytes);
    assert_eq!(read, entries);
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn concurrent_open_for_append_writes_one_header() {
    const WRITERS: usize = 8;

    let dir = scratch_dir("race");
    let path = dir.join("mtld3d_shaders.bin");
    // Every writer reaches the cold cache in the same instant, which is what a
    // set of devices whose first miss-compiles coincide does.
    let start = std::sync::Barrier::new(WRITERS);
    std::thread::scope(|scope| {
        for key in 0..WRITERS {
            let path = path.as_path();
            let start = &start;
            scope.spawn(move || {
                let key = u64::try_from(key).expect("writer index fits u64");
                start.wait();
                append_one(
                    path,
                    &CacheEntry {
                        source: None,
                        kind: CachedKind::Sm2Ps,
                        key,
                        body: EntryBody::Msl {
                            emitter: SHADER_EMITTER_VERSION,
                            msl: format!("fragment float4 ps{key}() {{ return float4({key}); }}"),
                        },
                    },
                );
            });
        }
    });
    let bytes = std::fs::read(&path).expect("read cache file");
    assert_eq!(magic_count(&bytes), 1);
    assert_eq!(read_header(&bytes), Ok(CacheHeader::CURRENT));
    let (read, _) = read_shaders(&bytes);
    assert_eq!(read.len(), WRITERS);
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn compact_renames_one_dense_file_into_place() {
    let dir = scratch_dir("bundle");
    let path = dir.join("mtld3d_shaders.bin");
    let entries = sample_entries();
    for entry in &entries {
        append_one(&path, entry);
    }
    let (_, len) = compact(&path)
        .expect("compact cache")
        .expect("cache needed compaction");
    let bytes = std::fs::read(&path).expect("read cache file");
    assert_eq!(bytes.len(), len);
    assert_eq!(magic_count(&bytes), 1);
    let (read, needs_compaction) = read_shaders(&bytes);
    assert_eq!(read, entries);
    assert!(!needs_compaction);
    // The temporary is renamed rather than left beside the cache file.
    let leftovers = std::fs::read_dir(&dir).expect("list scratch dir").count();
    assert_eq!(leftovers, 2);
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn append_and_compaction_race_does_not_lose_records() {
    const ROUNDS: u64 = 16;

    let dir = scratch_dir("compact-race");
    let path = dir.join("mtld3d_shaders.bin");
    append_one(&path, &sample_entries()[0]);
    for round in 0..ROUNDS {
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let cache_path = path.as_path();
            let barrier = &start;
            scope.spawn(move || {
                barrier.wait();
                compact(cache_path).expect("compact raced cache");
            });
            scope.spawn(move || {
                barrier.wait();
                append_one(
                    cache_path,
                    &CacheEntry {
                        source: None,
                        kind: CachedKind::Sm2Ps,
                        key: round + 1,
                        body: EntryBody::Msl {
                            emitter: SHADER_EMITTER_VERSION,
                            msl: format!("fragment float4 ps{round}() {{ return 1; }}"),
                        },
                    },
                );
            });
        });
    }
    let bytes = std::fs::read(&path).expect("read raced cache");
    let records = read_records(&bytes);
    assert_eq!(
        records.shaders.len(),
        usize::try_from(ROUNDS).expect("round count fits usize") + 1
    );
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn concurrent_loaders_and_writers_observe_valid_records() {
    const WRITERS: u64 = 2;
    const ROUNDS: u64 = 32;

    let dir = scratch_dir("load-write-race");
    let path = dir.join("mtld3d_shaders.bin");
    append_one(&path, &sample_entries()[0]);
    let start = std::sync::Barrier::new(usize::try_from(WRITERS).expect("writer count fits") + 1);
    std::thread::scope(|scope| {
        for writer in 0..WRITERS {
            let cache_path = path.as_path();
            let barrier = &start;
            scope.spawn(move || {
                barrier.wait();
                for round in 0..ROUNDS {
                    append_one(
                        cache_path,
                        &CacheEntry {
                            source: None,
                            kind: CachedKind::Sm3Ps,
                            key: 0x10_0000 + writer * ROUNDS + round,
                            body: EntryBody::Msl {
                                emitter: SHADER_EMITTER_VERSION,
                                msl: format!(
                                    "fragment float4 ps{writer}_{round}() {{ return 1; }}"
                                ),
                            },
                        },
                    );
                }
            });
        }
        let cache_path = path.as_path();
        let barrier = &start;
        scope.spawn(move || {
            barrier.wait();
            for _ in 0..ROUNDS {
                let CacheLoad::Current(records) = load(cache_path).expect("load raced cache")
                else {
                    panic!("raced cache stopped being current");
                };
                assert_ne!(records.shaders, [] as [CacheEntry; 0]);
            }
        });
    });
    let CacheLoad::Current(records) = load(&path).expect("load completed cache") else {
        panic!("completed cache stopped being current");
    };
    assert_eq!(
        records.shaders.len(),
        usize::try_from(WRITERS * ROUNDS + 1).expect("record count fits")
    );
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn load_invalidates_stale_versions_under_lock() {
    let dir = scratch_dir("stale");
    let path = dir.join("mtld3d_shaders.bin");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&SHADER_CACHE_MAGIC);
    bytes.extend_from_slice(&(CACHE_FORMAT_VERSION - 1).to_le_bytes());
    bytes.extend_from_slice(&SHADER_CACHE_SCHEMA_VERSION.to_le_bytes());
    std::fs::write(&path, bytes).expect("write stale cache");
    assert!(matches!(
        load(&path).expect("load stale cache"),
        CacheLoad::InvalidatedVersion(_)
    ));
    assert!(!path.exists());
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

fn programmable_entry(kind: CachedKind) -> CacheEntry {
    use crate::dxso::{VariantFlags, VariantKey, VsSamplerKinds, parse};

    if kind.is_air() {
        return air_entry(kind);
    }
    let major = match kind.msl_twin() {
        CachedKind::Sm1Vs | CachedKind::Sm1Ps => 1,
        CachedKind::Sm2Vs | CachedKind::Sm2Ps => 2,
        CachedKind::Sm3Vs | CachedKind::Sm3Ps => 3,
        _ => panic!("programmable fixture only"),
    };
    let header = if kind.is_vertex() {
        0xFFFE_0000
    } else {
        0xFFFF_0000
    };
    let minor = u32::from(major == 1);
    let program = parse(&[header | (major << 8) | minor, 0x0000_FFFF]).expect("parse DXSO");
    let source = if kind.is_vertex() {
        ShaderSource::vertex(
            &program,
            0xA55A,
            3,
            VsSamplerKinds {
                volume_mask: 1,
                cube_mask: 2,
                lod_table: true,
            },
        )
    } else {
        ShaderSource::pixel(
            &program,
            VariantKey {
                linked_input_mask: 0xA5,
                alpha_func: 5,
                fog_mode: 4,
                fog_table_mode: 3,
                depth_sampler_mask: 0x1234,
                fetch4_mask: 0x10,
                fetch4_alpha_mask: 0x10,
                raw_depth_red_mask: 0x20,
                depth_fetch_mask: 0x0034,
                volume_sampler_mask: 0x4000,
                cube_sampler_mask: 0x8000,
                tt_projected_mask: 0x85,
                color_out_mask: 0x0F,
                sample_mask: 0x5A,
                flags: VariantFlags::all(),
            },
        )
    };
    let key = source.disk_key();
    let msl = source.emit(&kind.entry_name(key)).expect("emit fixture");
    CacheEntry::new(kind, key, msl, Some(source))
}

#[test]
fn emitter_change_retains_and_rebuilds_every_programmable_stage_and_model() {
    for kind in [
        CachedKind::Sm1Vs,
        CachedKind::Sm1Ps,
        CachedKind::Sm2Vs,
        CachedKind::Sm2Ps,
        CachedKind::Sm3Vs,
        CachedKind::Sm3Ps,
    ] {
        let expected = programmable_entry(kind);
        let mut stale = expected.clone();
        flip_emitter(&mut stale);
        stale.set_msl("obsolete MSL must never compile".into());
        for bundle in [false, true] {
            let bytes = write_file(&[vec![stale.clone()]], bundle);
            let mut records = read_records(&bytes);
            assert_eq!(records.shaders.len(), 1);
            let entry = &mut records.shaders[0];
            assert_eq!(entry.source, expected.source, "all emission inputs survive");
            let tokens = entry.source().map(ShaderSource::tokens);
            assert_eq!(
                tokens,
                expected.source().map(ShaderSource::tokens),
                "the accessor hands back the retained DXSO"
            );
            assert_eq!(tokens.and_then(<[u32]>::last), Some(&0x0000_FFFF));
            assert!(entry.refresh_msl().expect("rebuild stale MSL"));
            assert_eq!(*entry, expected, "same specialization, name, and key");
            assert!(!entry.refresh_msl().expect("current MSL reuses its source"));
        }
    }
}

#[test]
fn emitter_change_discards_ff_and_only_its_dependent_pipelines() {
    let vs = programmable_entry(CachedKind::Sm3Vs);
    let ps = programmable_entry(CachedKind::Sm3Ps);
    let mut programmable = sample_recipe();
    programmable.vs = ShaderRecordRef::new(vs.kind, vs.key);
    programmable.ps = ShaderRecordRef::new(ps.kind, ps.key);
    let mut mixed = sample_recipe();
    mixed.vs = programmable.vs;
    mixed.ps = ShaderRecordRef::new(CachedKind::FfPs, 42);
    let mut entries = vec![
        vs,
        ps,
        CacheEntry::new(CachedKind::FfPs, 42, "old FF".into(), None),
    ];
    for entry in &mut entries {
        flip_emitter(entry);
    }
    let mut bytes = Vec::new();
    write_header(&mut bytes);
    write_bundle(&mut bytes, &entries, &[programmable, mixed]);
    let records = read_records(&bytes);
    assert_eq!(records.shaders.len(), 2);
    assert_eq!(records.pipelines.len(), 1);
    assert!(records.pipelines[0].ps.kind.is_programmable());
    assert!(records.needs_compaction);
}

#[test]
fn refreshed_msl_wins_in_either_order_and_compaction_preserves_concurrent_appends() {
    let dir = scratch_dir("emitter-refresh");
    let path = dir.join("mtld3d_shaders.bin");
    let expected = programmable_entry(CachedKind::Sm3Vs);
    let mut stale = expected.clone();
    flip_emitter(&mut stale);
    stale.set_msl("stale".into());
    for current_first in [false, true] {
        let entries = if current_first {
            vec![expected.clone(), stale.clone()]
        } else {
            vec![stale.clone(), expected.clone()]
        };
        let bytes = write_file(&[entries], false);
        assert_eq!(
            read_records(&bytes).shaders.as_slice(),
            std::slice::from_ref(&expected)
        );
    }
    append_one(&path, &stale);
    let CacheLoad::Current(mut loaded) = load(&path).expect("load old emitter") else {
        panic!("emitter changes do not invalidate the file");
    };
    assert!(loaded.shaders[0].refresh_msl().expect("refresh"));
    let other = programmable_entry(CachedKind::Sm2Ps);
    append_one(&path, &other);
    append_one(&path, &loaded.shaders[0]);
    // An older build may append after the refreshed record, too.
    append_one(&path, &stale);
    compact(&path)
        .expect("compact merged file")
        .expect("rewrite needed");
    let CacheLoad::Current(loaded) = load(&path).expect("reload") else {
        panic!("current file");
    };
    assert_eq!(loaded.shaders, [expected, other]);
    assert!(!loaded.needs_compaction);
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn malformed_source_and_wrong_identity_are_rejected() {
    let entry = programmable_entry(CachedKind::Sm3Vs);
    let mut body = Vec::new();
    entry.encode(&mut body);
    assert!(CacheEntry::decode(entry.kind, entry.key ^ 1, &body).is_none());
    assert!(CacheEntry::decode(CachedKind::Sm3Ps, entry.key, &body).is_none());
    assert!(CacheEntry::decode(CachedKind::FfVs, entry.key, &body).is_none());
    // Header + presence + VS specialization + DXSO count cannot be torn.
    for length in 0..27 {
        assert!(CacheEntry::decode(entry.kind, entry.key, &body[..length]).is_none());
    }
    // A forged token count cannot allocate past the encoded body.
    body[15..19].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(CacheEntry::decode(entry.kind, entry.key, &body).is_none());
}

#[test]
fn regeneration_failure_preserves_dxso_for_retry() {
    let mut entry = programmable_entry(CachedKind::Sm3Vs);
    // A framed record can contain DXSO a newer parser refuses.
    let tokens = [0xFFFE_0300, 0x0000_DEAD, 0x0000_FFFF];
    let mut body = Vec::new();
    flip_emitter(&mut entry);
    entry.encode(&mut body);
    body[15..19].copy_from_slice(&3u32.to_le_bytes());
    body.splice(23..23, 0x0000_DEADu32.to_le_bytes());
    let key = vs_source_disk_key_programmable(
        crate::ids::ProgramId::from_tokens(&tokens),
        0xA55A,
        3,
        crate::dxso::VsSamplerKinds {
            volume_mask: 1,
            cube_mask: 2,
            lod_table: true,
        },
    );
    let entry = CacheEntry::decode(entry.kind, key, &body).expect("framed DXSO source");
    let before = entry.clone();
    let bytes = write_file(&[vec![entry]], true);
    let mut loaded = read_records(&bytes);
    assert!(loaded.shaders[0].refresh_msl().is_err());
    assert_eq!(loaded.shaders[0], before);
}

#[test]
fn retained_dxso_accepts_the_parsers_end_opcode_encoding() {
    use crate::dxso::{VariantKey, VsSamplerKinds, parse};

    for (header, kind) in [
        (0xFFFE_0200, CachedKind::Sm2Vs),
        (0xFFFF_0200, CachedKind::Sm2Ps),
    ] {
        for end in [0x0000_FFFF, 0x0100_FFFF, 0x8000_FFFF] {
            // Shader creation and parsing identify END by its low opcode bits.
            let program = parse(&[header, end]).expect("accepted shader bytecode");
            let source = if kind.is_vertex() {
                ShaderSource::vertex(&program, 0, 0, VsSamplerKinds::default())
            } else {
                ShaderSource::pixel(&program, VariantKey::default())
            };
            let key = source.disk_key();
            let expected = source
                .emit(&kind.entry_name(key))
                .expect("emit accepted shader");
            let mut entry = CacheEntry::new(kind, key, "stale MSL".into(), Some(source));
            flip_emitter(&mut entry);
            let mut records = read_records(&write_file(&[vec![entry]], true));
            assert_eq!(
                records.shaders.len(),
                1,
                "accepted END {end:#x} must survive caching"
            );
            assert!(
                records.shaders[0]
                    .refresh_msl()
                    .expect("regenerate accepted shader")
            );
            assert_eq!(records.shaders[0].msl(), Some(expected.as_str()));
        }
    }
}

#[test]
fn inventory_counts_stored_records_across_emitter_fingerprints() {
    for emitter in [SHADER_EMITTER_VERSION, SHADER_EMITTER_VERSION ^ 1] {
        let mut entries = sample_entries();
        for entry in &mut entries {
            set_emitter(entry, emitter);
        }
        let mut recipe = sample_recipe();
        recipe.ps = ShaderRecordRef::new(entries[1].kind, entries[1].key);
        let mut bytes = Vec::new();
        write_header(&mut bytes);
        write_bundle(&mut bytes, &entries, &[recipe]);
        let stats = read_stats(&bytes).expect("valid stored inventory");
        assert_eq!(
            (stats.shaders, stats.ff_shaders, stats.pipelines),
            (3, 1, 1)
        );
        assert!(!stats.needs_compaction);
        if emitter != SHADER_EMITTER_VERSION {
            let runtime = read_records(&bytes);
            assert_eq!(runtime.shaders, [] as [CacheEntry; 0]);
            assert!(runtime.pipelines.is_empty());
            assert!(runtime.needs_compaction);
        }
    }
}

#[test]
fn inventory_rejects_checksum_schema_and_payload_damage() {
    let original = write_file(&[sample_entries()], true);
    let mut damaged = original.clone();
    damaged[HEADER_LEN + 16] ^= 1;
    assert!(read_stats(&damaged).is_err());
    let mut schema = original.clone();
    schema[12] ^= 1;
    assert!(read_stats(&schema).is_err());
    let mut format = original.clone();
    format[8] ^= 1;
    assert!(read_stats(&format).is_err());
    assert!(read_stats(&original[..original.len() - 1]).is_err());

    let mut malformed = Vec::new();
    write_header(&mut malformed);
    let frame = zstd::encode_all(&[0xff][..], ZSTD_APPEND_LEVEL).unwrap();
    push_chunk(&mut malformed, RECORD_KIND_BUNDLE, 0, &frame);
    assert!(read_stats(&malformed).is_err());
}

/// The AIR twin of `programmable_entry(kind.msl_twin())`, with a fake metallib.
fn air_entry(kind: CachedKind) -> CacheEntry {
    let msl = programmable_entry(kind.msl_twin());
    CacheEntry::new_air(
        kind,
        msl.key,
        [0xC2; 32],
        vec![0x4D, 0x54, 0x4C, 0x42, 1, 2, 3],
        msl.source().expect("programmable source").clone(),
    )
}

fn air_kinds() -> [CachedKind; 6] {
    [
        CachedKind::AirSm1Vs,
        CachedKind::AirSm1Ps,
        CachedKind::AirSm2Vs,
        CachedKind::AirSm2Ps,
        CachedKind::AirSm3Vs,
        CachedKind::AirSm3Ps,
    ]
}

fn air_body(entry: &CacheEntry) -> Vec<u8> {
    let mut body = Vec::new();
    entry.encode(&mut body);
    body
}

#[test]
fn air_record_round_trips() {
    for kind in air_kinds() {
        let entry = air_entry(kind);
        assert!(entry.msl().is_none());
        assert_eq!(
            entry.air().map(|(t, m)| (*t, m.len())),
            Some(([0xC2; 32], 7))
        );
        let decoded = CacheEntry::decode(kind, entry.key, &air_body(&entry)).expect("decode");
        assert_eq!(decoded, entry);
        let (read, _) = read_shaders(&write_file(&[vec![entry.clone()]], false));
        assert_eq!(read, [entry]);
    }
}

#[test]
fn air_record_survives_compaction() {
    let entries: Vec<_> = air_kinds().into_iter().map(air_entry).collect();
    let path = scratch_dir("air-compact").join("c.bin");
    for entry in &entries {
        append_one(&path, entry);
    }
    compact(&path).expect("compact");
    let CacheLoad::Current(records) = load(&path).expect("load") else {
        panic!("current cache");
    };
    assert_eq!(records.shaders, entries);
    assert!(!records.needs_compaction);
}

#[test]
fn air_record_with_a_short_body_is_dropped() {
    let entry = air_entry(CachedKind::AirSm3Ps);
    let body = air_body(&entry);
    // Every truncation (including empty and mid-source) is refused without a panic;
    // the empty-metallib cut (source only) is refused too.
    for cut in 0..=body.len() - entry.air().expect("air").1.len() {
        assert!(
            CacheEntry::decode(entry.kind, entry.key, &body[..cut]).is_none(),
            "{cut}"
        );
    }
    assert!(CacheEntry::decode(entry.kind, entry.key, &body).is_some());
}

#[test]
fn air_record_whose_source_kind_mismatches_is_dropped() {
    let entry = air_entry(CachedKind::AirSm3Ps);
    let body = air_body(&entry);
    for wrong in [
        CachedKind::AirSm2Ps,
        CachedKind::AirSm3Vs,
        CachedKind::AirSm1Ps,
    ] {
        assert!(
            CacheEntry::decode(wrong, entry.key, &body).is_none(),
            "{wrong:?}"
        );
    }
}

#[test]
fn air_record_whose_key_mismatches_its_source_is_dropped() {
    let entry = air_entry(CachedKind::AirSm3Vs);
    assert!(CacheEntry::decode(entry.kind, entry.key ^ 1, &air_body(&entry)).is_none());
}

#[test]
fn a_later_air_record_beats_its_earlier_twin_across_compaction() {
    let first = air_entry(CachedKind::AirSm2Ps);
    let mut second = first.clone();
    second.body = EntryBody::Air {
        translator: [0x77; 32],
        metallib: vec![9, 9, 9],
    };
    let path = scratch_dir("air-last-wins").join("c.bin");
    append_one(&path, &first);
    append_one(&path, &second);
    compact(&path).expect("compact");
    let CacheLoad::Current(records) = load(&path).expect("load") else {
        panic!("current cache");
    };
    assert_eq!(records.shaders, [second.clone()]);
    // Inside one bundle too: the later record wins.
    let (read, _) = read_shaders(&write_file(&[vec![second, first.clone()]], true));
    assert_eq!(read, [first]);
}

#[test]
fn air_record_never_needs_msl_regeneration() {
    let mut entry = air_entry(CachedKind::AirSm1Vs);
    assert!(!entry.needs_regeneration());
    assert_eq!(entry.refresh_msl(), Ok(false));
    entry.set_msl("ignored".into());
    assert!(entry.msl().is_none());
    let (read, _) = read_shaders(&write_file(&[vec![entry.clone()]], false));
    assert_eq!(read, [entry]);
}

fn recipe_of(vs: &CacheEntry, ps: &CacheEntry) -> PipelineRecipe {
    let mut recipe = sample_recipe();
    recipe.vs = ShaderRecordRef::new(vs.kind, vs.key);
    recipe.ps = ShaderRecordRef::new(ps.kind, ps.key);
    recipe
}

#[test]
fn a_recipe_of_two_air_stages_round_trips() {
    let vertex = air_entry(CachedKind::AirSm3Vs);
    let pixel = air_entry(CachedKind::AirSm3Ps);
    let recipe = recipe_of(&vertex, &pixel);
    assert!(recipe.is_valid());
    let mut bytes = Vec::new();
    write_header(&mut bytes);
    write_bundle(&mut bytes, &[vertex, pixel], std::slice::from_ref(&recipe));
    let records = read_records(&bytes);
    assert!(records.pipelines == [recipe]);
}

#[test]
fn a_recipe_mixing_air_and_msl_is_invalid() {
    let air_vertex = air_entry(CachedKind::AirSm3Vs);
    let msl_pixel = programmable_entry(CachedKind::Sm3Ps);
    assert!(!recipe_of(&air_vertex, &msl_pixel).is_valid());
    let msl_vertex = programmable_entry(CachedKind::Sm3Vs);
    let air_pixel = air_entry(CachedKind::AirSm3Ps);
    assert!(!recipe_of(&msl_vertex, &air_pixel).is_valid());
    assert!(recipe_of(&msl_vertex, &msl_pixel).is_valid());
}

#[test]
fn a_bundle_of_msl_and_air_records_loads() {
    let mut entries = sample_entries();
    entries.extend(air_kinds().into_iter().map(air_entry));
    let (read, needs_compaction) = read_shaders(&write_file(&[entries.clone()], true));
    assert_eq!(read, entries);
    assert!(!needs_compaction);
    let mut bytes = Vec::new();
    write_header(&mut bytes);
    write_bundle(&mut bytes, &entries, &[]);
    let stats = read_stats(&bytes).expect("stats");
    assert_eq!((stats.shaders, stats.ff_shaders), (9, 1));
}

/// A lit, fogged, clipped fixed-function vertex key reading two coordinate sets.
fn ff_vs_key() -> crate::dxso::FfVsKey {
    use crate::dxso::{FfVsFlags, FfVsKey, tci_entry};

    FfVsKey {
        flags: FfVsFlags::HAS_NORMAL | FfVsFlags::HAS_COLOR0 | FfVsFlags::LIGHTING_ENABLED,
        input_tex_coord_count: 2,
        tex_coord_count: 2,
        light_active_mask: 0b11,
        light_directional_mask: 0b01,
        light_spot_mask: 0b10,
        diffuse_source: 0,
        ambient_source: 0,
        specular_source: 0,
        emissive_source: 0,
        fog_mode: 3,
        tci: [tci_entry(0, 0), tci_entry(0, 1), 0, 0, 0, 0, 0, 0],
        passthrough: [0; 8],
        tex_coord_dims: [2, 3, 0, 0, 0, 0, 0, 0],
        tt_flags: [0x02, 0, 0, 0, 0, 0, 0, 0],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 2,
        reserved: 0,
    }
}

/// A textured `MODULATE` stage over the diffuse colour, the specular add on.
fn ff_ps_key() -> crate::dxso::FfPsKey {
    use mtld3d_types::{D3DTA_DIFFUSE, D3DTA_TEXTURE, D3DTOP_DISABLE, D3DTOP_MODULATE};

    use crate::dxso::{FfPsKey, FfStage, FfStageFlags};

    let op = |value: u32| u8::try_from(value).expect("a D3D9 op fits u8");
    let mut stages = [FfStage {
        color_op: op(D3DTOP_DISABLE),
        alpha_op: op(D3DTOP_DISABLE),
        ..FfStage::default()
    }; 8];
    stages[0] = FfStage {
        color_op: op(D3DTOP_MODULATE),
        color_arg1: op(D3DTA_TEXTURE),
        color_arg2: op(D3DTA_DIFFUSE),
        alpha_op: op(D3DTOP_MODULATE),
        alpha_arg1: op(D3DTA_TEXTURE),
        alpha_arg2: op(D3DTA_DIFFUSE),
        flags: FfStageFlags::HAS_TEXTURE,
    };
    FfPsKey {
        stages,
        specular_add: true,
        tt_projected_mask: 0x01,
    }
}

fn ff_variant() -> crate::dxso::VariantKey {
    crate::dxso::VariantKey {
        fog_mode: 3,
        color_out_mask: 0x01,
        flags: crate::dxso::VariantFlags::FLAT_SHADE,
        ..crate::dxso::VariantKey::default()
    }
}

/// The fixed-function source the builders give `ff_vs_key()` or `ff_ps_key()`.
fn ff_source(pixel: bool) -> ShaderSource {
    if pixel {
        let key = ff_ps_key();
        ShaderSource::ff_pixel(&key, ff_variant(), &crate::ff_build::pixel(&key))
    } else {
        let key = ff_vs_key();
        ShaderSource::ff_vertex(&key, &crate::ff_build::vertex(&key))
    }
}

/// A record of fixed-function `kind` carrying `source`: `ff.rs`'s MSL, or a fake metallib for 8/9.
fn ff_entry(kind: CachedKind, source: ShaderSource) -> CacheEntry {
    let key = source.disk_key();
    if kind.is_air() {
        return CacheEntry::new_air(kind, key, [0xC3; 32], vec![0x4D, 0x54, 1, 2], source);
    }
    let msl = source.emit(&kind.entry_name(key)).expect("ff.rs emits");
    CacheEntry::new(kind, key, msl, Some(source))
}

fn ff_kinds() -> [CachedKind; 4] {
    [
        CachedKind::FfVs,
        CachedKind::FfPs,
        CachedKind::AirFfVs,
        CachedKind::AirFfPs,
    ]
}

fn ff_kind_entry(kind: CachedKind) -> CacheEntry {
    ff_entry(kind, ff_source(kind.is_pixel()))
}

/// The record decoded from its own body, and from a file of it.
fn assert_round_trips(entry: &CacheEntry) {
    let decoded = CacheEntry::decode(entry.kind, entry.key, &air_body(entry));
    assert_eq!(decoded.as_ref(), Some(entry), "{:?}", entry.kind);
    for bundle in [false, true] {
        let (read, _) = read_shaders(&write_file(&[vec![entry.clone()]], bundle));
        assert_eq!(
            read,
            std::slice::from_ref(entry),
            "{:?} bundle={bundle}",
            entry.kind
        );
    }
}

#[test]
fn an_ff_vs_source_round_trips() {
    use airconv_ir::dxso::spec::{StageSpec, VsSpec};

    let key = ff_vs_key();
    let program = crate::ff_build::vertex(&key);
    let source = ShaderSource::ff_vertex(&key, &program);
    assert_eq!(source.tokens(), &program.tokens[..]);
    assert_eq!(
        source.disk_key(),
        crate::draw_data::vs_source_disk_key_ff(&key)
    );
    assert_eq!(
        source.stage_spec(),
        StageSpec::Vs(VsSpec {
            provided_input_mask: program.provided_input_mask,
            clip_plane_count: 2,
            ..VsSpec::default()
        })
    );
    let entry = ff_entry(CachedKind::FfVs, source);
    assert_eq!(
        entry.msl(),
        Some(&*crate::dxso::emit_vs_ff_named(
            &key,
            &CachedKind::FfVs.entry_name(entry.key)
        ))
    );
    assert_round_trips(&entry);
}

#[test]
fn an_ff_ps_source_round_trips() {
    use airconv_ir::dxso::spec::StageSpec;

    let key = ff_ps_key();
    let program = crate::ff_build::pixel(&key);
    let source = ShaderSource::ff_pixel(&key, ff_variant(), &program);
    assert_eq!(source.tokens(), &program.tokens[..]);
    assert_eq!(
        source.disk_key(),
        crate::draw_data::ps_source_disk_key_ff(&key, ff_variant())
    );
    assert_eq!(source.stage_spec(), StageSpec::Ps(ff_variant()));
    let entry = ff_entry(CachedKind::FfPs, source);
    assert_eq!(
        entry.msl(),
        Some(&*crate::dxso::emit_ps_ff_named(
            &key,
            ff_variant(),
            &CachedKind::FfPs.entry_name(entry.key)
        ))
    );
    assert_round_trips(&entry);
}

#[test]
fn an_air_ff_record_round_trips() {
    let entries = [
        ff_kind_entry(CachedKind::AirFfVs),
        ff_kind_entry(CachedKind::AirFfPs),
    ];
    for entry in &entries {
        assert!(entry.msl().is_none() && entry.air().is_some());
        assert_round_trips(entry);
    }
    let path = scratch_dir("air-ff-compact").join("c.bin");
    for entry in &entries {
        append_one(&path, entry);
    }
    compact(&path).expect("compact");
    let CacheLoad::Current(records) = load(&path).expect("load") else {
        panic!("current cache");
    };
    assert_eq!(records.shaders, entries);
}

#[test]
fn an_ff_source_on_the_wrong_stage_kind_is_refused() {
    for kind in ff_kinds() {
        let entry = ff_kind_entry(kind);
        let body = air_body(&entry);
        let twin = |vertex: CachedKind, pixel: CachedKind| {
            if kind.is_vertex() { pixel } else { vertex }
        };
        let wrong = if kind.is_air() {
            [
                twin(CachedKind::AirFfVs, CachedKind::AirFfPs),
                twin(CachedKind::AirSm3Vs, CachedKind::AirSm3Ps),
                if kind.is_vertex() {
                    CachedKind::AirSm3Vs
                } else {
                    CachedKind::AirSm3Ps
                },
            ]
        } else {
            [
                twin(CachedKind::FfVs, CachedKind::FfPs),
                twin(CachedKind::Sm3Vs, CachedKind::Sm3Ps),
                if kind.is_vertex() {
                    CachedKind::Sm3Vs
                } else {
                    CachedKind::Sm3Ps
                },
            ]
        };
        for other in wrong {
            assert!(
                CacheEntry::decode(other, entry.key, &body).is_none(),
                "{kind:?} body read as {other:?}"
            );
        }
        assert!(
            CacheEntry::decode(kind, entry.key ^ 1, &body).is_none(),
            "{kind:?} key"
        );
    }
    // A programmable source on an FF kind is refused too.
    for (programmable, ff) in [
        (CachedKind::Sm3Vs, CachedKind::FfVs),
        (CachedKind::Sm3Ps, CachedKind::FfPs),
        (CachedKind::AirSm3Vs, CachedKind::AirFfVs),
        (CachedKind::AirSm3Ps, CachedKind::AirFfPs),
    ] {
        let entry = programmable_entry(programmable);
        assert!(
            CacheEntry::decode(ff, entry.key, &air_body(&entry)).is_none(),
            "{ff:?}"
        );
    }
}

#[test]
fn a_truncated_ff_source_is_refused() {
    for kind in ff_kinds() {
        let entry = ff_kind_entry(kind);
        let body = air_body(&entry);
        // MSL: every cut before the source ends; AIR: every cut before the metallib's first byte.
        let end = if kind.is_air() {
            body.len() - entry.air().expect("air").1.len() + 1
        } else {
            body.len() - entry.msl().expect("msl").len()
        };
        for cut in 0..end {
            assert!(
                CacheEntry::decode(kind, entry.key, &body[..cut]).is_none(),
                "{kind:?} cut {cut}"
            );
        }
        assert!(
            CacheEntry::decode(kind, entry.key, &body).is_some(),
            "{kind:?}"
        );
    }
}

#[test]
fn a_runtime_msl_ff_record_stays_sourceless() {
    // A C3b build encodes an FF record as `emitter | has_source = 0 | MSL`: the bytes stay.
    for kind in [CachedKind::FfVs, CachedKind::FfPs] {
        let msl = "vertex VsOut mtld3d_vs_ff_00000001() { /* ff */ }";
        let entry = CacheEntry::new(kind, 0x0123_4567_89AB_CDEF, msl.into(), None);
        let mut expected = SHADER_EMITTER_VERSION.to_le_bytes().to_vec();
        expected.push(0);
        expected.extend_from_slice(msl.as_bytes());
        assert_eq!(air_body(&entry), expected, "{kind:?}");
        let decoded = CacheEntry::decode(kind, entry.key, &expected).expect("decode");
        assert!(decoded.source().is_none());
        assert_eq!(decoded, entry);
    }
}

#[test]
fn a_corpus_msl_ff_record_with_a_source_loads() {
    for kind in [CachedKind::FfVs, CachedKind::FfPs] {
        let expected = ff_kind_entry(kind);
        let mut stale = expected.clone();
        flip_emitter(&mut stale);
        stale.set_msl("obsolete MSL must never compile".into());
        let (mut read, _) = read_shaders(&write_file(&[vec![stale]], false));
        assert_eq!(
            read.len(),
            1,
            "{kind:?}: a stale FF record with a source is kept"
        );
        let entry = &mut read[0];
        assert_eq!(entry.source(), expected.source());
        assert!(entry.refresh_msl().expect("ff.rs regenerates"));
        assert_eq!(
            *entry, expected,
            "{kind:?}: the MSL ff.rs emits for the key"
        );
    }
}

#[test]
fn a_hostile_ff_key_is_refused() {
    use crate::dxso::{FfStageFlags, FfVsFlags};

    let program = crate::ff_build::vertex(&ff_vs_key());
    let mutated = |name, mutate: fn(&mut crate::dxso::FfVsKey)| {
        let mut key = ff_vs_key();
        mutate(&mut key);
        (name, key)
    };
    let vertex_cases = [
        mutated("unknown flag", |k| {
            k.flags = FfVsFlags::from_bits_retain(1 << 10);
        }),
        mutated("input sets", |k| k.input_tex_coord_count = 9),
        mutated("stage sets", |k| k.tex_coord_count = 9),
        mutated("tci set", |k| k.tci[0] = 0x08),
        mutated("dims", |k| k.tex_coord_dims[1] = 5),
        mutated("passthrough code", |k| {
            k.passthrough = [0xE0, 0, 0, 0, 0, 0, 0, 0];
        }),
        mutated("blend count", |k| k.vertex_blend_count = 5),
        mutated("clip planes", |k| k.clip_plane_count = 7),
        mutated("reserved", |k| k.reserved = 1),
    ];
    for (name, key) in vertex_cases {
        let source = ShaderSource::ff_vertex(&key, &program);
        for kind in [CachedKind::FfVs, CachedKind::AirFfVs] {
            let entry = if kind.is_air() {
                CacheEntry::new_air(kind, source.disk_key(), [1; 32], vec![1], source.clone())
            } else {
                CacheEntry::new(kind, source.disk_key(), String::new(), Some(source.clone()))
            };
            assert!(
                CacheEntry::decode(kind, entry.key, &air_body(&entry)).is_none(),
                "{name} on {kind:?}"
            );
        }
    }
    let program = crate::ff_build::pixel(&ff_ps_key());
    let mut key = ff_ps_key();
    key.stages[3].flags = FfStageFlags::from_bits_retain(1 << 2);
    let source = ShaderSource::ff_pixel(&key, ff_variant(), &program);
    let entry = CacheEntry::new(
        CachedKind::FfPs,
        source.disk_key(),
        String::new(),
        Some(source),
    );
    assert!(CacheEntry::decode(entry.kind, entry.key, &air_body(&entry)).is_none());
    // `specular_add` follows the emitter (8 bytes), the source flag and eight 7-byte stages.
    let entry = ff_kind_entry(CachedKind::FfPs);
    let mut body = air_body(&entry);
    assert_eq!(body[8 + 1 + 8 * 7], 1);
    body[8 + 1 + 8 * 7] = 2;
    assert!(CacheEntry::decode(entry.kind, entry.key, &body).is_none());
}

#[test]
fn a_warned_ff_key_round_trips() {
    use crate::dxso::{FfVsFlags, tci_entry};

    let mut key = ff_vs_key();
    key.flags |= FfVsFlags::COLOR_VERTEX;
    key.diffuse_source = 3;
    key.tci[1] = tci_entry(6, 1);
    let source = ShaderSource::ff_vertex(&key, &crate::ff_build::vertex(&key));
    for kind in [CachedKind::FfVs, CachedKind::AirFfVs] {
        assert_round_trips(&ff_entry(kind, source.clone()));
    }
}

#[test]
fn an_accepted_extreme_ff_key_builds_without_a_panic() {
    use crate::dxso::{FfPsKey, FfStage, FfStageFlags, FfVsFlags, FfVsKey, tci_entry};

    // Every field at the most the decoder accepts: a prewarm rebuild must not panic on it.
    // Extra semantics, then codes a vertex declaration never passes through (TEXCOORD0, COLOR0,
    // COLOR1, PSIZE0, POSITIONT0, a repeat), which the decoder accepts as nameable.
    let extras = [0xD0, 0x6F, 0x7F, 0x8F, 0x31, 0x22, 0x13, 0xC1];
    let aliases = [0x50, 0xA0, 0xA1, 0x40, 0x90, 0x50, 0, 0];
    for (flags, passthrough) in [
        (FfVsFlags::all(), extras),
        (FfVsFlags::all(), aliases),
        (FfVsFlags::all() - FfVsFlags::HAS_RHW, extras),
    ] {
        let key = FfVsKey {
            flags,
            input_tex_coord_count: 8,
            tex_coord_count: 8,
            light_active_mask: 0xFF,
            light_directional_mask: 0x0F,
            light_spot_mask: 0xF0,
            diffuse_source: 0xFF,
            ambient_source: 0xFF,
            specular_source: 0xFF,
            emissive_source: 0xFF,
            fog_mode: 0xFF,
            // Modes 15 down to 9, then SPHEREMAP; every stage reads set 7.
            tci: [15, 14, 13, 12, 11, 10, 9, 4].map(|mode| tci_entry(mode, 7)),
            passthrough,
            tex_coord_dims: [4; 8],
            tt_flags: [0xFF, 0x17, 0x14, 0x13, 0x12, 0x07, 0x04, 0x10],
            vertex_blend_count: 4,
            declared_weights_count: 0xFF,
            clip_plane_count: 6,
            reserved: 0,
        };
        let program = crate::ff_build::vertex(&key);
        crate::dxso::parse(&program.tokens).expect("the extreme key's tokens parse");
        let source = ShaderSource::ff_vertex(&key, &program);
        assert_round_trips(&ff_entry(CachedKind::FfVs, source));
    }
    let key = FfPsKey {
        stages: [FfStage {
            color_op: 0xFF,
            color_arg1: 0xFF,
            color_arg2: 0xFF,
            alpha_op: 0xFF,
            alpha_arg1: 0xFF,
            alpha_arg2: 0xFF,
            flags: FfStageFlags::all(),
        }; 8],
        specular_add: true,
        tt_projected_mask: 0xFF,
    };
    let program = crate::ff_build::pixel(&key);
    crate::dxso::parse(&program.tokens).expect("the extreme key's tokens parse");
    let source = ShaderSource::ff_pixel(&key, ff_variant(), &program);
    assert_round_trips(&ff_entry(CachedKind::FfPs, source));
}

#[test]
fn a_rebuilt_ff_source_is_none_when_current() {
    for pixel in [false, true] {
        assert_eq!(ff_source(pixel).rebuilt_ff(), None);
    }
    let programmable = programmable_entry(CachedKind::Sm3Vs);
    assert_eq!(
        programmable.source().and_then(ShaderSource::rebuilt_ff),
        None
    );
    // Another key's build stored under this key: the rebuild is this key's, mask included.
    let key = ff_vs_key();
    let mut other = key.clone();
    other.flags.remove(crate::dxso::FfVsFlags::HAS_NORMAL);
    other.input_tex_coord_count = 0;
    let stale_program = crate::ff_build::vertex(&other);
    let fresh_program = crate::ff_build::vertex(&key);
    assert_ne!(
        stale_program.provided_input_mask,
        fresh_program.provided_input_mask
    );
    let stale = ShaderSource::ff_vertex(&key, &stale_program);
    assert_eq!(
        stale.rebuilt_ff(),
        Some(ShaderSource::ff_vertex(&key, &fresh_program))
    );
}

#[test]
fn a_32_bit_builds_ff_record_decodes_on_a_64_bit_host() {
    use crate::shader_key::ff_key_hash_32;

    for kind in ff_kinds() {
        let mut entry = ff_kind_entry(kind);
        entry.key = if kind.is_pixel() {
            ff_key_hash_32(&(&ff_ps_key(), ff_variant()))
        } else {
            ff_key_hash_32(&ff_vs_key())
        };
        assert_round_trips(&entry);
        assert!(
            CacheEntry::decode(kind, entry.key ^ 1, &air_body(&entry)).is_none(),
            "{kind:?}"
        );
    }
}
