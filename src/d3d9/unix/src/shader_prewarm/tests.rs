use std::path::{Path, PathBuf};

use mtld3d_core::{
    dxso::{VariantKey, VsSamplerKinds, parse},
    pipeline_state::{
        ExtraColorAttachments, PipelineAttachFlags, PipelineRsBits, PipelineSnapshot, StreamLayout,
    },
    shader_cache::{PipelineRecipe, ShaderSource},
};
use mtld3d_shared::{
    VertexAttrDesc,
    mtl::{PixelFormat, VertexFormat, VertexStepFunction},
};

use super::*;

/// `vs_2_0`: `dcl_position v0` / `mov oPos, v0`.
const VS_2_0: [u32; 8] = [
    0xFFFE_0200,
    0x0200_001F,
    0x8000_0000,
    0x900F_0000,
    0x0200_0001,
    0xC00F_0000,
    0x90E4_0000,
    0x0000_FFFF,
];

/// `ps_2_0`: `def c0, 1, 0, 0, 1` / `mov oC0, c0`.
const PS_2_0: [u32; 11] = [
    0xFFFF_0200,
    0x0500_0051,
    0xA00F_0000,
    0x3F80_0000,
    0,
    0,
    0x3F80_0000,
    0x0200_0001,
    0x800F_0800,
    0xA0E4_0000,
    0x0000_FFFF,
];

const KEY: [u8; 32] = [0xC2; 32];
const OTHER_KEY: [u8; 32] = [0x11; 32];

/// The AIR record of a stage translated now, stamped with `translator`.
fn air_entry(pixel: bool, translator: [u8; 32]) -> CacheEntry {
    let (tokens, kind): (&[u32], _) = if pixel {
        (&PS_2_0, CachedKind::AirSm2Ps)
    } else {
        (&VS_2_0, CachedKind::AirSm2Vs)
    };
    let program = parse(tokens).expect("the test program parses");
    let source = if pixel {
        ShaderSource::pixel(&program, VariantKey::default())
    } else {
        ShaderSource::vertex(&program, 1, 0, VsSamplerKinds::default())
    };
    let key = source.disk_key();
    let metallib = translate_stage(&source, &kind.entry_name(key)).expect("translates");
    CacheEntry::new_air(kind, key, translator, metallib, source)
}

/// A cache holding the two AIR records stamped `translator` and the recipe of their pair.
fn air_cache(name: &str, translator: [u8; 32]) -> (PathBuf, [ShaderRecordRef; 2]) {
    let dir = std::env::temp_dir().join(format!("mtld3d-prewarm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("mtld3d_shaders.bin");
    let (vs, ps) = (air_entry(false, translator), air_entry(true, translator));
    let refs = [
        ShaderRecordRef::new(vs.kind, vs.key),
        ShaderRecordRef::new(ps.kind, ps.key),
    ];
    let mut stream_layouts = [StreamLayout::UNUSED; mtld3d_types::MAX_STREAMS as usize];
    stream_layouts[0] = StreamLayout {
        stride: 16,
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
    let recipe = PipelineRecipe::from_snapshot(
        refs[0],
        refs[1],
        &snapshot,
        &[VertexAttrDesc {
            attr_index: 0,
            buffer_index: 0,
            offset: 0,
            format: VertexFormat::Float4,
        }],
    );
    let writer = shader_cache::CacheWriter::open(&path).expect("cache file");
    writer.append_shader(&vs).expect("append VS");
    writer.append_shader(&ps).expect("append PS");
    writer.append_pipeline(&recipe).expect("append recipe");
    (path, refs)
}

/// A record reference as its kind byte and key, for comparing and printing.
const fn plain(reference: ShaderRecordRef) -> (u8, u64) {
    (reference.kind() as u8, reference.key())
}

fn records(path: &Path) -> shader_cache::CacheRecords {
    match shader_cache::load(path).expect("readable") {
        CacheLoad::Current(records) => records,
        _ => panic!("the cache is current"),
    }
}

fn device() -> crate::mtl::objc2::rc::Retained<
    crate::mtl::objc2::runtime::ProtocolObject<dyn crate::mtl::objc2_metal::MTLDevice>,
> {
    crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device")
}

fn handle_of(
    device: &crate::mtl::objc2::rc::Retained<
        crate::mtl::objc2::runtime::ProtocolObject<dyn crate::mtl::objc2_metal::MTLDevice>,
    >,
) -> MetalHandle<MTLDeviceKind> {
    // SAFETY: the device's retain outlives every use of the handle in these tests.
    unsafe { MetalHandle::new(crate::mtl::objc2::rc::Retained::as_ptr(device) as u64) }
}

fn prewarm(
    device: MetalHandle<MTLDeviceKind>,
    path: &Path,
    air_key: Option<[u8; 32]>,
) -> WarmCache {
    run(
        device,
        &AtomicBool::new(false),
        true,
        Some(path.to_path_buf()),
        air_key,
    )
    .expect("prewarm reads the cache")
}

#[test]
fn prewarm_skips_air_records_and_recipes_under_msl() {
    assert_eq!(air_key(ShaderBackend::Msl, Some(KEY)), None);
    assert_eq!(air_key(ShaderBackend::Air, None), None);
    assert_eq!(air_key(ShaderBackend::Air, Some(KEY)), Some(KEY));
    for kind in [
        CachedKind::AirSm1Vs,
        CachedKind::AirSm2Ps,
        CachedKind::AirSm3Ps,
    ] {
        assert!(skipped(kind, None), "{kind:?} is left unread without a key");
        assert!(!skipped(kind, Some(&KEY)));
    }
    for kind in [CachedKind::FfVs, CachedKind::Sm2Vs, CachedKind::Sm3Ps] {
        assert!(!skipped(kind, None));
    }
    let (path, _) = air_cache("msl", KEY);
    // No device: an AIR record admitted under msl would reach Metal and fail.
    let warm = prewarm(MetalHandle::NULL, &path, None);
    assert!(warm.libraries.is_empty() && warm.pipelines.is_empty());
    let kept = records(&path);
    assert_eq!(
        (kept.shaders.len(), kept.pipelines.len()),
        (2, 1),
        "the AIR records and their recipe stay in the file"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn prewarm_builds_air_records_under_air() {
    let device = device();
    let (path, refs) = air_cache("air", KEY);
    let warm = prewarm(handle_of(&device), &path, Some(KEY));
    let mut built: Vec<_> = warm
        .libraries
        .iter()
        .map(|(reference, _)| plain(*reference))
        .collect();
    built.sort_unstable();
    assert_eq!(built, refs.map(plain));
    assert_eq!(warm.pipelines.len(), 1, "the AIR pair's recipe builds");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn prewarm_regenerates_an_air_record_with_another_key() {
    let device = device();
    let (path, refs) = air_cache("regen", KEY);
    let warm = prewarm(handle_of(&device), &path, Some(OTHER_KEY));
    assert_eq!(warm.libraries.len(), 2);
    assert_eq!(warm.pipelines.len(), 1);
    let kept = records(&path);
    let mut stamped: Vec<_> = kept
        .shaders
        .iter()
        .map(|entry| {
            (
                plain(ShaderRecordRef::new(entry.kind, entry.key)),
                entry.air().map(|(key, _)| *key),
            )
        })
        .collect();
    stamped.sort_unstable();
    assert_eq!(
        stamped,
        [
            (plain(refs[0]), Some(OTHER_KEY)),
            (plain(refs[1]), Some(OTHER_KEY))
        ],
        "compaction keeps the regenerated records only"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
