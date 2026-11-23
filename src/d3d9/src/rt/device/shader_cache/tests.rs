use core::sync::atomic::Ordering;
use std::{
    sync::{Arc, PoisonError},
    thread,
};

use airconv_ir::dxso::spec::{StageSpec, VariantFlags, VariantKey, VsSamplerKinds, VsSpec};
use dxmt_core::{
    pipeline_cache::{Compiled, PsoMap},
    shader_cache::Store,
};
use mtld3d_core::{
    dxso::{FfPsKey, FfStage, FfStageFlags, FfVsFlags, FfVsKey},
    ids::ProgramId,
    pipeline_key::{
        CoreExtra, DeclCode, MAX_STREAMS, PipelineKey, PsStageKey, ShaderCode, StageSource,
        StreamKey, VertexStepFunction, VsStageKey,
    },
    pipeline_state::{BlendFactor, BlendOperation, CoreBlend},
};
use mtld3d_types::{
    D3DDECL_END, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_FLOAT4, D3DDECLTYPE_UNUSED,
    D3DDECLUSAGE_POSITION, D3DVERTEXELEMENT9,
};
use winemetal_sys::Obj;

use super::{
    CompileStats, RecipeRecorder, StageFunctions, decode_recipes, encode_recipes, load_recipes,
    shader_cache_version, stage_cache_key,
};
use crate::rt::device::fixture::FAKE_GLOBAL_LOCK;

fn fake_device() -> Obj {
    // SAFETY: made-up non-zero handle only `dxmt-fakes` inspects.
    unsafe { Obj::adopt(0x0c01) }.expect("non-zero handle")
}

fn prog_code(tokens: &[u32]) -> ShaderCode {
    ShaderCode::from_parts(ProgramId::from_tokens(tokens), Arc::from(tokens))
}

#[test]
fn stage_cache_key_is_deterministic_and_domain_disjoint() {
    let tokens = [0xfffe_0200, 0x0000_ffff];
    let vs_spec = StageSpec::Vs(VsSpec {
        provided_input_mask: 0x3,
        clip_plane_count: 1,
        sampler_kinds: VsSamplerKinds {
            volume_mask: 1,
            cube_mask: 2,
            lod_table: true,
        },
    });
    let ps_spec = StageSpec::Ps(VariantKey::default());

    let k1 = stage_cache_key(&tokens, &vs_spec, c"d3d9_vs", 310);
    let k2 = stage_cache_key(&tokens, &vs_spec, c"d3d9_vs", 310);
    assert_eq!(k1, k2);

    let k_other_stage = stage_cache_key(&tokens, &ps_spec, c"d3d9_vs", 310);
    let k_other_name = stage_cache_key(&tokens, &vs_spec, c"d3d9_ps", 310);
    let k_other_ver = stage_cache_key(&tokens, &vs_spec, c"d3d9_vs", 320);
    let k_other_tok = stage_cache_key(&[0xfffe_0300, 0x0000_ffff], &vs_spec, c"d3d9_vs", 310);

    assert_ne!(k1, k_other_stage);
    assert_ne!(k1, k_other_name);
    assert_ne!(k1, k_other_ver);
    assert_ne!(k1, k_other_tok);
}

#[test]
fn stage_functions_sqlite_cold_then_warm_hit_and_concurrent_dedup() {
    let _guard = FAKE_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    dxmt_core::shader_cache::close_all();
    dxmt_fakes::reset_shader_cache();

    let version = shader_cache_version();
    let store = Arc::new(Store::new(31099, version));
    assert!(store.has_reader() && store.has_writer());

    let dev = fake_device();
    let stats = Arc::new(CompileStats::default());
    let funcs = Arc::new(StageFunctions::with_cache(
        Some(Arc::clone(&store)),
        Arc::clone(&stats),
    ));
    let stage = Arc::new(StageSource {
        code: prog_code(&[0xfffe_0200, 0x0000_ffff]),
        spec: StageSpec::Vs(VsSpec::default()),
    });

    // Two threads race on the exact same stage: only 1 compile/miss occurs, both get the same handle.
    let dev_ref = &dev;
    let (h1, h2) = thread::scope(|s| {
        let f1 = Arc::clone(&funcs);
        let s1 = Arc::clone(&stage);
        let t1 = s.spawn(move || f1.function(dev_ref, 31099, &s1, c"d3d9_vs"));
        let f2 = Arc::clone(&funcs);
        let s2 = Arc::clone(&stage);
        let t2 = s.spawn(move || f2.function(dev_ref, 31099, &s2, c"d3d9_vs"));
        (t1.join().unwrap(), t2.join().unwrap())
    });
    assert!(h1.is_some());
    assert_eq!(h1, h2);
    assert_eq!(stats.stage_compiles.load(Ordering::Relaxed), 1);
    assert_eq!(stats.cache_misses.load(Ordering::Relaxed), 1);
    assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 0);

    // A fresh device table backed by the same SQLite store hits the cached metallib with 0 compiles.
    let warm_stats = Arc::new(CompileStats::default());
    let warm_funcs = StageFunctions::with_cache(Some(store), Arc::clone(&warm_stats));
    let h3 = warm_funcs.function(&dev, 31099, &stage, c"d3d9_vs");
    assert!(h3.is_some());
    assert_eq!(warm_stats.cache_hits.load(Ordering::Relaxed), 1);
    assert_eq!(warm_stats.cache_misses.load(Ordering::Relaxed), 0);
    assert_eq!(warm_stats.stage_compiles.load(Ordering::Relaxed), 0);
}

fn sample_prog_key(custom_end: bool) -> PipelineKey {
    let pos = D3DVERTEXELEMENT9 {
        stream: 0,
        offset: 0,
        type_: D3DDECLTYPE_FLOAT4,
        method: D3DDECLMETHOD_DEFAULT,
        usage: D3DDECLUSAGE_POSITION,
        usage_index: 0,
    };
    let end = if custom_end {
        D3DVERTEXELEMENT9 {
            stream: 0xff,
            offset: 0,
            type_: D3DDECLTYPE_UNUSED,
            method: 0,
            usage: 0,
            usage_index: 9,
        }
    } else {
        D3DDECL_END
    };
    let vs_code = prog_code(&[0xfffe_0200, 0x0000_ffff]);
    let ps_code = prog_code(&[0xffff_0200, 0x0000_ffff]);
    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = StreamKey::Bound {
        stride: 16,
        step: VertexStepFunction::PerVertex,
        rate: 1,
        shift: 2,
    };
    streams[1] = StreamKey::Null;
    streams[2] = StreamKey::Inline { stride: 32 };
    PipelineKey {
        vs: VsStageKey::Prog {
            code: vs_code,
            clip_plane_count: 2,
            sampler_kinds: VsSamplerKinds {
                volume_mask: 1,
                cube_mask: 4,
                lod_table: true,
            },
        },
        ps: PsStageKey::Prog(ps_code),
        variant: VariantKey {
            alpha_func: 5,
            fog_mode: 1,
            fog_table_mode: 2,
            linked_input_mask: 0x3,
            depth_sampler_mask: 0x4,
            depth_fetch_mask: 0x4,
            fetch4_mask: 0x1,
            fetch4_alpha_mask: 0x1,
            raw_depth_red_mask: 0x4,
            volume_sampler_mask: 0x2,
            cube_sampler_mask: 0x8,
            tt_projected_mask: 0x1,
            color_out_mask: 0x2,
            sample_mask: 0xf,
            flags: VariantFlags::FLAT_SHADE | VariantFlags::SRGB_WRITE,
        },
        layout: DeclCode::new(Arc::from([pos, end])),
        streams,
        write_mask: 0x7,
        color_format: 80,
        depth_format: 252,
        blend: Some(CoreBlend {
            src: BlendFactor::SourceAlpha,
            dst: BlendFactor::OneMinusSourceAlpha,
            op: BlendOperation::Add,
            src_alpha: BlendFactor::One,
            dst_alpha: BlendFactor::Zero,
            op_alpha: BlendOperation::Max,
        }),
        extras: [
            Some(CoreExtra {
                format: 80,
                write_mask: 0xf,
                blend: None,
            }),
            None,
            None,
        ],
        samples: 4,
        alpha_to_coverage: true,
    }
}

fn sample_ff_key() -> PipelineKey {
    let pos = D3DVERTEXELEMENT9 {
        stream: 0,
        offset: 0,
        type_: D3DDECLTYPE_FLOAT4,
        method: D3DDECLMETHOD_DEFAULT,
        usage: D3DDECLUSAGE_POSITION,
        usage_index: 0,
    };
    PipelineKey {
        vs: VsStageKey::Ff(FfVsKey {
            flags: FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED,
            input_tex_coord_count: 1,
            tex_coord_count: 2,
            light_active_mask: 0x3,
            light_directional_mask: 0x1,
            light_spot_mask: 0x2,
            diffuse_source: 1,
            ambient_source: 0,
            specular_source: 2,
            emissive_source: 0,
            fog_mode: 3,
            tci: [0, 1, 2, 3, 4, 5, 6, 7],
            passthrough: [1, 2, 0, 0, 0, 0, 0, 0],
            tex_coord_dims: [2, 3, 0, 0, 0, 0, 0, 0],
            tt_flags: [0x13, 0, 0, 0, 0, 0, 0, 0],
            vertex_blend_count: 2,
            declared_weights_count: 1,
            clip_plane_count: 1,
            reserved: 0,
        }),
        ps: PsStageKey::Ff(FfPsKey {
            stages: [FfStage {
                color_op: 4,
                color_arg1: 2,
                color_arg2: 0,
                alpha_op: 2,
                alpha_arg1: 2,
                alpha_arg2: 0,
                flags: FfStageFlags::HAS_TEXTURE | FfStageFlags::RESULT_TEMP,
            }; 8],
            specular_add: true,
            tt_projected_mask: 0x5,
        }),
        variant: VariantKey::default(),
        layout: DeclCode::new(Arc::from([pos, D3DDECL_END])),
        streams: [StreamKey::Unused; MAX_STREAMS as usize],
        write_mask: 0xf,
        color_format: 80,
        depth_format: 0,
        blend: None,
        extras: [None, None, None],
        samples: 1,
        alpha_to_coverage: false,
    }
}

#[test]
fn recipe_codec_roundtrips_canonical_and_non_canonical_keys_and_hits_pso_map() {
    let k1 = sample_prog_key(false);
    let k2 = sample_prog_key(true);
    let k3 = sample_ff_key();

    let encoded = encode_recipes(&[k1.clone(), k2.clone(), k3.clone()]);
    let decoded = decode_recipes(&encoded);
    assert_eq!(decoded, vec![k1.clone(), k2.clone(), k3.clone()]);
    assert_eq!(
        decoded[1].layout.elements_with_end(),
        k2.layout.elements_with_end()
    );

    // Prewarmed decoded keys must hit `PsoMap::get` when queried by live draw keys.
    let map = PsoMap::<PipelineKey, ()>::default();
    for (idx, k) in decoded.into_iter().enumerate() {
        let h = u64::try_from(idx + 1).unwrap() * 0x100;
        let _ = map.insert_ready(k, Compiled::ready_with(Arc::new(()), h, None));
    }
    assert_eq!(map.get(&k1).and_then(|e| e.ready()), Some(0x100));
    assert_eq!(map.get(&k2).and_then(|e| e.ready()), Some(0x200));
    assert_eq!(map.get(&k3).and_then(|e| e.ready()), Some(0x300));
}

#[test]
fn recipe_codec_rejects_corrupt_and_truncated_blobs_without_panicking() {
    assert_eq!(decode_recipes(b""), Vec::new());
    assert_eq!(
        decode_recipes(b"BADMAGIC\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"),
        Vec::new()
    );

    let valid = encode_recipes(&[sample_ff_key()]);
    assert_ne!(decode_recipes(&valid), Vec::new());
    assert_eq!(decode_recipes(&valid[..valid.len() - 1]), Vec::new());

    let mut with_trailing = valid;
    with_trailing.push(0xaa);
    assert_eq!(decode_recipes(&with_trailing), Vec::new());
}

#[test]
fn recipe_recorder_union_merges_existing_on_flush() {
    let _guard = FAKE_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    dxmt_core::shader_cache::close_all();
    dxmt_fakes::reset_shader_cache();

    let version = shader_cache_version();
    let store = Store::new(31098, version);
    let k1 = sample_prog_key(false);
    let k2 = sample_ff_key();

    // First session records k1 and flushes.
    let mut rec1 = RecipeRecorder::seed(Vec::new());
    rec1.record(&k1);
    assert_eq!(rec1.flush(&store, 31098), 1);

    // Concurrent/subsequent session was seeded empty (simulating race/cold seed) and records k2.
    let mut rec2 = RecipeRecorder::seed(Vec::new());
    rec2.record(&k2);
    assert_eq!(rec2.flush(&store, 31098), 2);

    let loaded = load_recipes(&store, 31098);
    assert_eq!(loaded.len(), 2);
    assert!(loaded.contains(&k1));
    assert!(loaded.contains(&k2));
}
