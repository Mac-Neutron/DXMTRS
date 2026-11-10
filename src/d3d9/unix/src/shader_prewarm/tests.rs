use std::path::{Path, PathBuf};

use mtld3d_core::{
    dxso::{VariantKey, VsSamplerKinds, parse},
    pipeline_state::{
        ExtraColorAttachments, PipelineAttachFlags, PipelineRsBits, PipelineSnapshot, StreamLayout,
    },
    shader_cache::PipelineRecipe,
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
        CachedKind::AirFfVs,
        CachedKind::AirFfPs,
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

/// A lit fixed-function vertex key; `normal` adds the normal input and one coordinate set.
fn ff_vs_key(normal: bool) -> mtld3d_core::dxso::FfVsKey {
    use mtld3d_core::dxso::{FfVsFlags, FfVsKey};

    let mut flags = FfVsFlags::HAS_COLOR0;
    if normal {
        flags |= FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED;
    }
    FfVsKey {
        flags,
        input_tex_coord_count: u8::from(normal),
        tex_coord_count: u8::from(normal),
        light_active_mask: u8::from(normal),
        light_directional_mask: u8::from(normal),
        light_spot_mask: 0,
        diffuse_source: 0,
        ambient_source: 0,
        specular_source: 0,
        emissive_source: 0,
        fog_mode: 0,
        tci: [0; 8],
        passthrough: [0; 8],
        tex_coord_dims: [2, 0, 0, 0, 0, 0, 0, 0],
        tt_flags: [0; 8],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 0,
        reserved: 0,
    }
}

/// A fixed-function pixel key: `textured` modulates a texture into the diffuse colour.
fn ff_ps_key(textured: bool) -> mtld3d_core::dxso::FfPsKey {
    use mtld3d_core::dxso::{FfPsKey, FfStage, FfStageFlags};
    use mtld3d_types::{
        D3DTA_DIFFUSE, D3DTA_TEXTURE, D3DTOP_DISABLE, D3DTOP_MODULATE, D3DTOP_SELECTARG1,
    };

    let code = |value: u32| u8::try_from(value).expect("a D3D9 code fits u8");
    let disabled = FfStage {
        color_op: code(D3DTOP_DISABLE),
        alpha_op: code(D3DTOP_DISABLE),
        ..FfStage::default()
    };
    let mut stages = [disabled; 8];
    stages[0] = if textured {
        FfStage {
            color_op: code(D3DTOP_MODULATE),
            color_arg1: code(D3DTA_TEXTURE),
            color_arg2: code(D3DTA_DIFFUSE),
            alpha_op: code(D3DTOP_MODULATE),
            alpha_arg1: code(D3DTA_TEXTURE),
            alpha_arg2: code(D3DTA_DIFFUSE),
            flags: FfStageFlags::HAS_TEXTURE,
        }
    } else {
        FfStage {
            color_op: code(D3DTOP_SELECTARG1),
            color_arg1: code(D3DTA_DIFFUSE),
            alpha_op: code(D3DTOP_SELECTARG1),
            alpha_arg1: code(D3DTA_DIFFUSE),
            ..FfStage::default()
        }
    };
    FfPsKey {
        stages,
        specular_add: false,
        tt_projected_mask: 0,
    }
}

/// The AIR record of `source`, translated now and stamped `translator`.
fn ff_air_entry(source: ShaderSource, translator: [u8; 32]) -> CacheEntry {
    let kind = if matches!(
        source.stage_spec(),
        airconv_ir::dxso::spec::StageSpec::Vs(_)
    ) {
        CachedKind::AirFfVs
    } else {
        CachedKind::AirFfPs
    };
    let key = source.disk_key();
    let metallib = translate_stage(&source, &kind.entry_name(key)).expect("translates");
    CacheEntry::new_air(kind, key, translator, metallib, source)
}

/// The current fixed-function sources of `ff_vs_key(true)` and `ff_ps_key(true)`.
fn fresh_ff_sources() -> [ShaderSource; 2] {
    let (vs, ps) = (ff_vs_key(true), ff_ps_key(true));
    [
        ShaderSource::ff_vertex(&vs, &mtld3d_core::ff_build::vertex(&vs)),
        ShaderSource::ff_pixel(
            &ps,
            VariantKey::default(),
            &mtld3d_core::ff_build::pixel(&ps),
        ),
    ]
}

#[test]
fn stale_ff_tokens_regenerate_the_air_record() {
    let device = device();
    let dir = std::env::temp_dir().join(format!("mtld3d-prewarm-ff-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("mtld3d_shaders.bin");
    // Each key stored with another key's build: other tokens, and for the VS another input mask.
    let (vs, ps) = (ff_vs_key(true), ff_ps_key(true));
    let stale = [
        ShaderSource::ff_vertex(&vs, &mtld3d_core::ff_build::vertex(&ff_vs_key(false))),
        ShaderSource::ff_pixel(
            &ps,
            VariantKey::default(),
            &mtld3d_core::ff_build::pixel(&ff_ps_key(false)),
        ),
    ];
    let writer = shader_cache::CacheWriter::open(&path).expect("cache file");
    let mut refs = Vec::new();
    for source in stale {
        let entry = ff_air_entry(source, KEY);
        refs.push(plain(ShaderRecordRef::new(entry.kind, entry.key)));
        writer.append_shader(&entry).expect("append");
    }
    let warm = prewarm(handle_of(&device), &path, Some(KEY));
    let mut built: Vec<_> = warm.libraries.iter().map(|(r, _)| plain(*r)).collect();
    built.sort_unstable();
    refs.sort_unstable();
    assert_eq!(built, refs);
    let kept = records(&path);
    assert_eq!(
        kept.shaders.len(),
        2,
        "compaction keeps the regenerated records only"
    );
    for (entry, fresh) in kept.shaders.iter().zip(fresh_ff_sources()) {
        assert_eq!(
            entry.source(),
            Some(&fresh),
            "{:?}: this key's build",
            entry.kind
        );
        assert_eq!(entry.air().map(|(key, _)| *key), Some(KEY));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn matching_ff_tokens_keep_the_record() {
    let device = device();
    for source in fresh_ff_sources() {
        let entry = ff_air_entry(source, KEY);
        let kept = compile_library(handle_of(&device), &entry, Some(&KEY));
        assert!(kept.handles.is_some(), "{:?} builds", entry.kind);
        assert!(
            kept.regenerated.is_none(),
            "{:?}: current tokens",
            entry.kind
        );
        // Another translator build regenerates it as any AIR record, from the same source.
        let other = compile_library(handle_of(&device), &entry, Some(&OTHER_KEY));
        assert!(other.handles.is_some());
        let regenerated = other.regenerated.expect("another key regenerates");
        assert_eq!(regenerated.air().map(|(key, _)| *key), Some(OTHER_KEY));
        assert_eq!(regenerated.source(), entry.source());
        assert_eq!(regenerated.key, entry.key);
    }
}
