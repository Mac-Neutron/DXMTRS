use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
    sync::Arc,
};

use airconv_ir::dxso::spec::{StageSpec, VsSamplerKinds, VsSpec};
use mtld3d_shared::mtl::{PixelFormat, VertexFormat};
use mtld3d_types::{
    D3DDECL_END, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_D3DCOLOR, D3DDECLTYPE_FLOAT2,
    D3DDECLTYPE_FLOAT3, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_COLOR, D3DDECLUSAGE_NORMAL,
    D3DDECLUSAGE_POSITION, D3DDECLUSAGE_POSITIONT, D3DDECLUSAGE_TEXCOORD, D3DFVF_DIFFUSE,
    D3DFVF_SPECULAR, D3DFVF_XYZRHW, D3DVERTEXELEMENT9, D3DVIEWPORT9, render_state_defaults,
};
use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::Xxh3;

use super::{
    CoreRefusal, DeclCode, DrawFacts, DrawFlags, PipelineKey, PipelineSource, PsStageKey,
    SampledMasks, ShaderCode, StageKind, VsStageKey, core_draw_kinds, core_key_depth,
    core_ps_variant, core_scissor, core_srgb_variant, core_viewport, float_rows,
    indexed_up_vertices, pipeline_source, pos_fixup, rhw_depth_clamp, up_byte_count,
    viewport_depth_range,
};
use crate::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements, rhw_passthrough},
    dxso::{
        FF_PASSTHROUGH_ATTR_BASE, FfVsKey, VariantFlags, VariantKey, ff_attr_index_for_semantic,
    },
    ff_state::FfState,
    ids::ProgramId,
};

/// `vs_2_0 { dcl_position v0; dcl_texcoord1 v3; mov oPos, v0; mov oT0, v3 }`.
const VS_POS_TEX1: [u32; 14] = [
    0xfffe_0200, // vs_2_0
    0x0200_001f,
    0x8000_0000,
    0x900f_0000, // dcl_position v0
    0x0200_001f,
    0x8001_0005,
    0x900f_0003, // dcl_texcoord1 v3
    0x0200_0001,
    0xc00f_0000,
    0x90e4_0000, // mov oPos, v0
    0x0200_0001,
    0xe00f_0000,
    0x90e4_0003, // mov oT0, v3
    0x0000_ffff,
];

/// `vs_2_0 { dcl_position v0; dcl_texcoord1 v3; mov oPos, v0; mov oT0, v0 }`: other tokens.
const VS_POS_TEX1_OTHER: [u32; 14] = [
    0xfffe_0200, // vs_2_0
    0x0200_001f,
    0x8000_0000,
    0x900f_0000, // dcl_position v0
    0x0200_001f,
    0x8001_0005,
    0x900f_0003, // dcl_texcoord1 v3
    0x0200_0001,
    0xc00f_0000,
    0x90e4_0000, // mov oPos, v0
    0x0200_0001,
    0xe00f_0000,
    0x90e4_0000, // mov oT0, v0
    0x0000_ffff,
];

/// `ps_2_0 { mov oC0, c0 }`.
const PS_C0: [u32; 5] = [
    0xffff_0200, // ps_2_0
    0x0200_0001,
    0x800f_0800,
    0xa0e4_0000, // mov oC0, c0
    0x0000_ffff,
];

/// `ps_2_0 { mov oC0, c1 }`.
const PS_C1: [u32; 5] = [
    0xffff_0200, // ps_2_0
    0x0200_0001,
    0x800f_0800,
    0xa0e4_0001, // mov oC0, c1
    0x0000_ffff,
];

/// One declaration element of the default method.
const fn element(stream: u16, offset: u16, type_: u8, usage: u8, index: u8) -> D3DVERTEXELEMENT9 {
    D3DVERTEXELEMENT9 {
        stream,
        offset,
        type_,
        method: D3DDECLMETHOD_DEFAULT,
        usage,
        usage_index: index,
    }
}

/// The declaration of `elements`, `D3DDECL_END` appended.
fn decl(elements: &[D3DVERTEXELEMENT9]) -> DeclCode {
    DeclCode::new(elements.iter().copied().chain([D3DDECL_END]).collect())
}

/// The program of `tokens`, in an `Arc` of its own.
fn code(tokens: &[u32]) -> ShaderCode {
    ShaderCode::from_parts(ProgramId::from_tokens(tokens), Arc::from(tokens))
}

/// The default-state fixed-function vertex key of `elements`, with their passthrough list.
fn ff_vs(elements: &[D3DVERTEXELEMENT9]) -> FfVsKey {
    let rs = render_state_defaults();
    let layout = ff_vs_layout_from_elements(elements);
    FfState::new().build_vs_key(&rs, layout, 0, rhw_passthrough(elements))
}

/// The key of a default-state `XYZRHW | DIFFUSE` fixed-function draw, every colour written.
fn ff_base() -> PipelineKey {
    let rs = render_state_defaults();
    let ff = FfState::new();
    let (elements, stride) = fvf_to_elements(D3DFVF_XYZRHW | D3DFVF_DIFFUSE);
    PipelineKey {
        vs: VsStageKey::Ff(ff_vs(&elements)),
        ps: PsStageKey::Ff(ff.build_ps_key(&rs, 0)),
        variant: ff.variant_key(&rs, true, false),
        layout: decl(&elements),
        stride,
        write_mask: 0xf,
        color_format: PixelFormat::Bgra8Unorm as u32,
        depth_format: 0,
        blend: None,
    }
}

/// POSITION0 FLOAT4 @0, TEXCOORD1 FLOAT2 @16, COLOR0 D3DCOLOR @24 (no shader here reads it).
fn prog_elements() -> [D3DVERTEXELEMENT9; 3] {
    [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
        element(0, 16, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        element(0, 24, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0),
    ]
}

/// The key of a programmable pair over [`prog_elements`], 28 bytes a vertex.
fn prog_base() -> PipelineKey {
    PipelineKey {
        vs: VsStageKey::Prog {
            code: code(&VS_POS_TEX1),
            clip_plane_count: 0,
            sampler_kinds: VsSamplerKinds::default(),
        },
        ps: PsStageKey::Prog(code(&PS_C0)),
        variant: VariantKey::default(),
        layout: decl(&prog_elements()),
        stride: 28,
        write_mask: 0xf,
        color_format: PixelFormat::Bgra8Unorm as u32,
        depth_format: 0,
        blend: None,
    }
}

fn source(key: &PipelineKey) -> PipelineSource {
    pipeline_source(key).expect("the key's stages build")
}

/// The vertex stage's provided-input mask in `source`.
fn provided(source: &PipelineSource) -> u16 {
    match source.vs.spec {
        StageSpec::Vs(spec) => spec.provided_input_mask,
        StageSpec::Ps(_) => panic!("a vertex spec on the vertex stage"),
    }
}

fn ff_vs_of(key: &mut PipelineKey) -> &mut FfVsKey {
    match &mut key.vs {
        VsStageKey::Ff(vs) => vs,
        VsStageKey::Prog { .. } => panic!("a fixed-function vertex stage"),
    }
}

fn prog_clip_of(key: &mut PipelineKey) -> &mut u8 {
    match &mut key.vs {
        VsStageKey::Prog {
            clip_plane_count, ..
        } => clip_plane_count,
        VsStageKey::Ff(_) => panic!("a programmable vertex stage"),
    }
}

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = Xxh3::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// One field of a key changed.
type Change = fn(&mut PipelineKey);

/// A change's name, the key it changes and the change.
type Case = (&'static str, fn() -> PipelineKey, Change);

/// What [`core_draw_kinds`] answers.
type Kinds = Result<(StageKind, StageKind), CoreRefusal>;

fn prog_kinds_of(key: &mut PipelineKey) -> &mut VsSamplerKinds {
    match &mut key.vs {
        VsStageKey::Prog { sampler_kinds, .. } => sampler_kinds,
        VsStageKey::Ff(_) => panic!("a programmable vertex stage"),
    }
}

/// A sampling vertex shader's kinds reach the lowering.
///
/// A cube or volume slot types its texture, and the LOD table makes `texldl` read its row.
#[test]
fn prog_vs_spec_carries_the_sampler_kinds() {
    let mut key = prog_base();
    *prog_kinds_of(&mut key) = VsSamplerKinds {
        volume_mask: 0b100,
        cube_mask: 0b10,
        lod_table: true,
    };
    match source(&key).vs.spec {
        StageSpec::Vs(spec) => {
            assert_eq!(spec.sampler_kinds.volume_mask, 0b100);
            assert_eq!(spec.sampler_kinds.cube_mask, 0b10);
            assert!(spec.sampler_kinds.lod_table);
        }
        StageSpec::Ps(_) => panic!("a vertex spec on the vertex stage"),
    }
    // The fixed-function vertex stage samples nothing.
    match source(&ff_base()).vs.spec {
        StageSpec::Vs(spec) => assert_eq!(spec.sampler_kinds, VsSamplerKinds::default()),
        StageSpec::Ps(_) => panic!("a vertex spec on the vertex stage"),
    }
}

/// Each field of the key, in each stage arm, reaches what its pipeline is built from.
///
/// Two keys differing in one field give different sources, so a key that left a pipeline input
/// out would bind one pipeline for two; equal keys give equal sources.
#[test]
fn pipeline_key_tracks_every_input() {
    assert_eq!(source(&ff_base()), source(&ff_base()));
    assert_eq!(source(&prog_base()), source(&prog_base()));
    let changes: [Case; 18] = [
        ("vs Ff (tokens)", ff_base, |k| {
            ff_vs_of(k).tex_coord_count = 1;
        }),
        ("vs Ff (clip planes)", ff_base, |k| {
            ff_vs_of(k).clip_plane_count = 1;
        }),
        ("vs Prog (tokens)", prog_base, |k| {
            k.vs = VsStageKey::Prog {
                code: code(&VS_POS_TEX1_OTHER),
                clip_plane_count: 0,
                sampler_kinds: VsSamplerKinds::default(),
            };
        }),
        ("vs Prog (clip planes)", prog_base, |k| *prog_clip_of(k) = 2),
        ("vs Prog (a cube vertex sampler)", prog_base, |k| {
            prog_kinds_of(k).cube_mask = 0b10;
        }),
        ("vs Prog (the vertex LOD table)", prog_base, |k| {
            prog_kinds_of(k).lod_table = true;
        }),
        ("vs arm", prog_base, |k| {
            k.vs = VsStageKey::Ff(ff_vs(&prog_elements()));
        }),
        ("ps Ff", ff_base, |k| {
            if let PsStageKey::Ff(ps) = &mut k.ps {
                ps.specular_add = true;
            }
        }),
        ("ps Prog", prog_base, |k| {
            k.ps = PsStageKey::Prog(code(&PS_C1));
        }),
        ("ps arm", prog_base, |k| {
            let rs = render_state_defaults();
            k.ps = PsStageKey::Ff(FfState::new().build_ps_key(&rs, 0));
        }),
        ("variant", ff_base, |k| k.variant.alpha_func = 5),
        ("layout (FF)", ff_base, |k| {
            let fvf = D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_SPECULAR;
            k.layout = decl(&fvf_to_elements(fvf).0);
        }),
        ("layout (Prog)", prog_base, |k| {
            let mut elements = prog_elements();
            elements[1].offset = 20;
            k.layout = decl(&elements);
        }),
        ("stride", ff_base, |k| k.stride = 24),
        ("write_mask", ff_base, |k| k.write_mask = 0x7),
        ("color_format", ff_base, |k| {
            k.color_format = PixelFormat::Rgba16Unorm as u32;
        }),
        ("depth_format", ff_base, |k| {
            k.depth_format = PixelFormat::Depth32Float as u32;
        }),
        ("blend", ff_base, |k| {
            let mut rs = render_state_defaults();
            rs[mtld3d_types::D3DRS_ALPHABLENDENABLE as usize] = 1;
            k.blend = crate::pipeline_state::core_blend(&rs, true);
        }),
    ];
    for (field, base, change) in changes {
        let mut key = base();
        change(&mut key);
        assert_ne!(key, base(), "{field}");
        assert_ne!(source(&key), source(&base()), "{field}");
    }
}

/// Programs are equal by their tokens, never by an id alone, and hash by the id.
#[test]
fn shader_code_equality_is_exact() {
    let a = code(&PS_C0);
    let b = code(&PS_C0);
    assert!(!Arc::ptr_eq(&a.tokens, &b.tokens));
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
    assert_eq!(a.clone(), a);
    let forged = ShaderCode::from_parts(ProgramId::from_tokens(&PS_C0), Arc::from(&PS_C1[..]));
    assert_ne!(a, forged);
    assert_ne!(a, code(&PS_C1));
}

/// Declarations are equal by their elements; `elements` leaves `D3DDECL_END` out.
#[test]
fn decl_code_equality_is_exact() {
    let a = decl(&prog_elements());
    let b = decl(&prog_elements());
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
    assert_eq!(a.elements(), &prog_elements()[..]);
    let mut other = prog_elements();
    other[2].usage_index = 1;
    assert_ne!(a, decl(&other));
}

/// A programmable vertex stage takes the elements its `dcl`s name, by semantic, from stream 0.
///
/// The unconsumed COLOR0 past them is no attribute and leaves the extent at 24.
#[test]
fn programmable_attrs_resolve_by_semantic() {
    let source = source(&prog_base());
    assert_eq!(
        source.attrs,
        [
            (0, VertexFormat::Float4 as u32, 0),
            (3, VertexFormat::Float2 as u32, 16)
        ]
    );
    assert_eq!(
        source.vs.spec,
        StageSpec::Vs(VsSpec {
            provided_input_mask: 0b1001,
            clip_plane_count: 0,
            sampler_kinds: VsSamplerKinds::default(),
        })
    );
    assert_eq!(source.stream0_extent, 24);
    assert_eq!(source.stride, 28);
    assert_eq!(source.ps.spec, StageSpec::Ps(VariantKey::default()));
}

/// A UP draw feeds stream 0 only: an element on another stream provides no input to either stage.
#[test]
fn elements_off_stream_0_are_not_provided() {
    let prog = PipelineKey {
        layout: decl(&[
            element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
            element(1, 0, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        ]),
        stride: 16,
        ..prog_base()
    };
    let prog = source(&prog);
    assert_eq!(provided(&prog), 0b0001);
    assert_eq!(prog.attrs, [(0, VertexFormat::Float4 as u32, 0)]);

    let ff_mask = |elements: &[D3DVERTEXELEMENT9]| {
        let key = PipelineKey {
            vs: VsStageKey::Ff(ff_vs(elements)),
            layout: decl(elements),
            stride: 20,
            ..ff_base()
        };
        provided(&source(&key))
    };
    let position = element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITIONT, 0);
    let color0 = 1 << ff_attr_index_for_semantic(D3DDECLUSAGE_COLOR, 0).expect("an attribute");
    let on_0 = ff_mask(&[
        position,
        element(0, 16, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0),
    ]);
    assert_ne!(on_0 & color0, 0, "the FF VS reads COLOR0 from stream 0");
    let on_1 = ff_mask(&[
        position,
        element(1, 0, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0),
    ]);
    assert_eq!(on_1 & color0, 0, "nor from stream 1");
    assert_ne!(on_1 & 1, 0, "the position stays provided");
}

/// A pre-transformed layout's extra semantic reaches the FF VS at its passthrough attribute.
#[test]
fn rhw_decl_feeds_the_ff_vs_with_its_passthrough() {
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITIONT, 0),
        element(0, 16, D3DDECLTYPE_FLOAT3, D3DDECLUSAGE_NORMAL, 0),
    ];
    let key = PipelineKey {
        vs: VsStageKey::Ff(ff_vs(&elements)),
        layout: decl(&elements),
        stride: 28,
        ..ff_base()
    };
    let source = source(&key);
    let passthrough = u32::from(FF_PASSTHROUGH_ATTR_BASE);
    assert!(
        source
            .attrs
            .contains(&(passthrough, VertexFormat::Float3 as u32, 16)),
        "{:?}",
        source.attrs
    );
    assert_ne!(provided(&source) & (1 << passthrough), 0);
}

/// An attribute ending past the stride crosses into the next vertex.
#[test]
fn a_crossing_attribute_is_detected() {
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
        element(0, 20, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
    ];
    let crossing = PipelineKey {
        layout: decl(&elements),
        stride: 16,
        ..prog_base()
    };
    assert!(source(&crossing).crosses_stride());
    let whole = PipelineKey {
        stride: 28,
        ..crossing
    };
    assert!(!source(&whole).crosses_stride());
    assert!(!source(&prog_base()).crosses_stride());
}

/// Core's pixel variant: one colour target, no samplers, one sample, its linked inputs.
///
/// A programmable stage keeps its depth output exactly when a depth-stencil is attached.
#[test]
fn core_ps_variant_rules() {
    let base = VariantKey {
        alpha_func: 5,
        color_out_mask: 0b110,
        sample_mask: 0b11,
        flags: VariantFlags::SRGB_WRITE
            | VariantFlags::POINT_SPRITE
            | VariantFlags::LOD_BIAS
            | VariantFlags::VPOS_SCALE
            | VariantFlags::SAMPLE_MASK,
        ..VariantKey::default()
    };
    let prog = core_ps_variant(base, true, false, 0b101, false);
    assert_eq!(prog.color_out_mask, 0);
    assert_eq!(prog.linked_input_mask, 0b101);
    assert_eq!(prog.alpha_func, 5);
    assert_eq!(prog.sample_mask, 0);
    assert_eq!(
        prog.flags,
        VariantFlags::SRGB_WRITE | VariantFlags::NO_DEPTH_ATTACHMENT
    );
    let points = core_ps_variant(base, true, true, 0, false);
    assert!(points.flags.contains(VariantFlags::POINT_SPRITE));
    let ff = core_ps_variant(base, false, false, 0, false);
    assert_eq!(ff.flags, VariantFlags::SRGB_WRITE);
    assert_eq!(ff.linked_input_mask, 0);
    // With a depth-stencil attached the stage exports its depth, even from a base that had not.
    let no_depth_base = VariantKey {
        flags: base.flags | VariantFlags::NO_DEPTH_ATTACHMENT,
        ..base
    };
    let with_depth = core_ps_variant(no_depth_base, true, false, 0, true);
    assert_eq!(with_depth.flags, VariantFlags::SRGB_WRITE);
    assert_eq!(
        core_ps_variant(base, false, false, 0, true).flags,
        VariantFlags::SRGB_WRITE
    );
}

/// Which stages a draw takes, or why the core refuses it.
#[test]
fn core_draw_kinds_cover_every_case() {
    use CoreRefusal::{BoundDepthSampled, NotCoreShader, VertexDepthTexture};
    use StageKind::{Ff, Prog};
    let facts = |flags| DrawFacts { flags };
    let both = DrawFlags::VS_BOUND | DrawFlags::VS_CORE | DrawFlags::PS_BOUND | DrawFlags::PS_CORE;
    let rhw = both | DrawFlags::LAYOUT_RHW;
    let cases: [(DrawFacts, Kinds); 10] = [
        (facts(both), Ok((Prog, Prog))),
        (facts(rhw), Ok((Ff, Prog))),
        // A pre-transformed layout bypasses the bound VS: none of its flags counts.
        (facts(rhw - DrawFlags::VS_CORE), Ok((Ff, Prog))),
        // No vertex shader: the fixed-function vertex stage, whatever the layout.
        (
            facts(both - DrawFlags::VS_BOUND - DrawFlags::VS_CORE),
            Ok((Ff, Prog)),
        ),
        (facts(DrawFlags::empty()), Ok((Ff, Ff))),
        (facts(DrawFlags::LAYOUT_RHW), Ok((Ff, Ff))),
        (facts(both - DrawFlags::PS_BOUND), Ok((Prog, Ff))),
        (facts(both - DrawFlags::VS_CORE), Err(NotCoreShader)),
        (facts(both - DrawFlags::PS_CORE), Err(NotCoreShader)),
        (
            facts(both | DrawFlags::VERTEX_DEPTH_TEXTURE),
            Err(VertexDepthTexture),
        ),
    ];
    for (i, (facts, expected)) in cases.into_iter().enumerate() {
        assert_eq!(core_draw_kinds(facts), expected, "case {i}");
    }
    let refusals = [NotCoreShader, VertexDepthTexture, BoundDepthSampled];
    for refusal in refusals {
        // Every refusal there is: one added or dropped does not compile until this names it.
        match refusal {
            NotCoreShader | VertexDepthTexture | BoundDepthSampled => {}
        }
    }
    let names = refusals.map(CoreRefusal::name);
    assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
}

/// The depth format a draw's key names, or why the core refuses the draw.
///
/// The bound depth-stencil's format when a pass can attach it and the draw does not sample it;
/// otherwise none for a draw that uses no plane, and a refusal for one that does.
#[test]
fn core_key_depth_covers_every_case() {
    const F: u32 = 260;
    // (bound, sampled by the draw, uses a plane) -> the key's format.
    let cases = [
        (None, false, true, Ok(0)),
        (None, false, false, Ok(0)),
        (None, true, true, Ok(0)),
        (Some(F), false, true, Ok(F)),
        (Some(F), false, false, Ok(F)),
        (Some(F), true, false, Ok(0)),
        (Some(F), true, true, Err(CoreRefusal::BoundDepthSampled)),
    ];
    for (bound, sampled, uses_plane, expected) in cases {
        assert_eq!(
            core_key_depth(bound, sampled, uses_plane),
            expected,
            "{bound:?} {sampled} {uses_plane}"
        );
    }
}

/// Relative addressing binds every row; otherwise the rows the program names.
#[test]
fn float_rows_bind_the_whole_file_for_relative() {
    assert_eq!(float_rows(12, false), 12);
    assert_eq!(float_rows(2, true), 256);
    assert_eq!(float_rows(0, false), 0);
}

/// An indexed UP draw's vertex range past `u32` is refused.
#[test]
fn indexed_byte_count_is_checked() {
    assert_eq!(indexed_up_vertices(2, 4), Some(6));
    assert_eq!(indexed_up_vertices(u32::MAX, 1), None);
    assert_eq!(indexed_up_vertices(u32::MAX, 0), Some(u32::MAX));
}

/// A zero-sized viewport covers the target; a depth range narrower than 0.001 is widened to it.
#[test]
fn core_viewport_rules() {
    let target = (64, 32);
    let unset = D3DVIEWPORT9 {
        x: 3,
        y: 2,
        width: 0,
        height: 16,
        min_z: 0.0,
        max_z: 1.0,
    };
    let bits = |v: [f64; 6]| v.map(f64::to_bits);
    assert_eq!(
        bits(core_viewport(&unset, target)),
        bits([0.0, 0.0, 64.0, 32.0, 0.0, 1.0])
    );
    let flat = D3DVIEWPORT9 {
        x: 8,
        y: 4,
        width: 16,
        height: 8,
        min_z: 0.5,
        max_z: 0.5,
    };
    let zfar = f64::from(0.5_f32 + 0.001);
    assert_eq!(
        bits(core_viewport(&flat, target)),
        bits([8.0, 4.0, 16.0, 8.0, 0.5, zfar])
    );
    let range = |(near, far): (f32, f32)| (near.to_bits(), far.to_bits());
    assert_eq!(range(viewport_depth_range(0.25, 0.75)), range((0.25, 0.75)));
    assert_eq!(
        range(viewport_depth_range(0.25, 0.0)),
        range((0.25, 0.25 + 0.001))
    );
}

/// A `stride * count` past `u32` is refused, on every host: no `DrawPrimitiveUP` passes 4 GiB.
#[test]
fn up_byte_count_refuses_an_overflow() {
    assert_eq!(up_byte_count(20, 3), Some(60));
    // Past `i32::MAX` on every host: on i686 the caller's slice may not exceed `isize::MAX`.
    assert_eq!(up_byte_count(0x7fff_ffff, 1), Some(0x7fff_ffff));
    assert_eq!(up_byte_count(0x8000_0000, 1), None);
    assert_eq!(up_byte_count(u32::MAX, 1), None);
    assert_eq!(up_byte_count(u32::MAX, 2), None);
    assert_eq!(up_byte_count(0x1_0000, 0x1_0000), None);
}

/// A device's programs of the same tokens share one `Arc`, its id their hash.
///
/// An entry under the id holding other tokens (a collision) or a dropped program is replaced,
/// never reused.
#[test]
fn shader_code_interning_is_exact() {
    let mut programs = FxHashMap::default();
    let first = ShaderCode::intern(&mut programs, &VS_POS_TEX1);
    let again = ShaderCode::intern(&mut programs, &VS_POS_TEX1);
    assert_eq!(first.tokens().as_ptr(), again.tokens().as_ptr());
    assert_eq!(first.id(), ProgramId::from_tokens(&VS_POS_TEX1));
    let other: Arc<[u32]> = Arc::from(&[0xffff_0200_u32, 0x0000_ffff][..]);
    programs.insert(first.id(), Arc::downgrade(&other));
    let collided = ShaderCode::intern(&mut programs, &VS_POS_TEX1);
    assert_eq!(collided.tokens(), VS_POS_TEX1);
    assert_eq!(collided, first);
    drop((first, again, collided));
    let fresh = ShaderCode::intern(&mut programs, &VS_POS_TEX1);
    assert_eq!(fresh.tokens(), VS_POS_TEX1);
    assert_eq!(programs.len(), 1);
}

/// The half-pixel fixup is the drawn viewport's; the clamp and the depth bias are the caller's.
#[test]
fn pos_fixup_follows_the_drawn_viewport() {
    let target = (256, 128);
    let viewport = |width, height| D3DVIEWPORT9 {
        x: 0,
        y: 0,
        width,
        height,
        min_z: 0.0,
        max_z: 1.0,
    };
    let bits = |v: [f32; 5]| v.map(f32::to_bits);
    assert_eq!(
        bits(pos_fixup(&viewport(128, 64), target, false, 0.0)),
        bits([1.0 / 128.0, -1.0 / 64.0, 0.0, 1.0, 0.0])
    );
    assert_eq!(
        bits(pos_fixup(&viewport(0, 64), target, true, -0.25)),
        bits([1.0 / 256.0, -1.0 / 128.0, 1.0, 1.0, -0.25])
    );
}

/// mtld3d's rule: a pre-transformed draw clamps its depth unless a depth test is live.
#[test]
fn rhw_depth_clamp_follows_mtld3d() {
    // (rhw, a depth attached, ZENABLE) -> clamp.
    let cases = [
        (true, false, false, true),
        (true, false, true, true),
        (true, true, false, true),
        (true, true, true, false),
        (false, false, false, false),
        (false, true, true, false),
    ];
    for (rhw, has_depth, depth_enable, clamp) in cases {
        assert_eq!(
            rhw_depth_clamp(rhw, has_depth, depth_enable),
            clamp,
            "{rhw} {has_depth} {depth_enable}"
        );
    }
}

/// A textured draw's variant types its volume and cube slots and takes `LOD_BIAS` with its table.
#[test]
fn texture_variant_types_slots_and_lod() {
    let masks = SampledMasks {
        volume: 0b10,
        cube: 0b100,
        ..SampledMasks::default()
    };
    let v = super::core_texture_variant(VariantKey::default(), &masks, true);
    assert_eq!((v.volume_sampler_mask, v.cube_sampler_mask), (0b10, 0b100));
    assert!(v.flags.contains(VariantFlags::LOD_BIAS));
    let v = super::core_texture_variant(v, &SampledMasks::default(), false);
    assert!(!v.flags.contains(VariantFlags::LOD_BIAS));
}

/// A draw's variant carries its depth slots (shadow and raw) and its fetch4 slots, as mtld3d's.
#[test]
fn core_texture_variant_carries_the_depth_and_fetch4_masks() {
    let masks = SampledMasks {
        volume: 0,
        cube: 0,
        depth: 0b0110,
        depth_fetch: 0b0100,
        raw_depth_red: 0b0100,
        fetch4: 0b1001,
        fetch4_alpha: 0b1000,
    };
    let v = super::core_texture_variant(VariantKey::default(), &masks, false);
    assert_eq!(v.depth_sampler_mask, 0b0110);
    assert_eq!(v.depth_fetch_mask, 0b0100);
    assert_eq!(v.raw_depth_red_mask, 0b0100);
    assert_eq!((v.fetch4_mask, v.fetch4_alpha_mask), (0b1001, 0b1000));
    // A draw without them clears what the base carried.
    let v = super::core_texture_variant(v, &SampledMasks::default(), false);
    assert_eq!(v, VariantKey::default());
}

#[test]
fn core_scissor_follows_mtld3d() {
    use mtld3d_types::D3DRECT;
    let rect = |x1, y1, x2, y2| D3DRECT { x1, y1, x2, y2 };
    let viewport = |x, y, width, height| D3DVIEWPORT9 {
        x,
        y,
        width,
        height,
        min_z: 0.0,
        max_z: 1.0,
    };
    let target = (64, 32);
    let full = viewport(0, 0, 64, 32);
    // The test on: the rect, whatever the viewport.
    assert_eq!(
        core_scissor(true, rect(8, 4, 40, 20), &full, target),
        [8, 4, 32, 16]
    );
    // A negative origin starts at 0; an inverted rect is empty at its origin.
    assert_eq!(
        core_scissor(true, rect(-8, -4, 40, 20), &full, target),
        [0, 0, 40, 20]
    );
    assert_eq!(
        core_scissor(true, rect(40, 20, 8, 4), &full, target),
        [40, 20, 0, 0]
    );
    // The test off: the drawn viewport, and the target for an unset one.
    assert_eq!(
        core_scissor(false, rect(8, 4, 40, 20), &viewport(4, 2, 16, 8), target),
        [4, 2, 16, 8]
    );
    assert_eq!(
        core_scissor(false, rect(8, 4, 40, 20), &viewport(4, 2, 0, 8), target),
        [0, 0, 64, 32]
    );
}

#[test]
fn core_srgb_variant_rules() {
    let base = VariantKey {
        flags: VariantFlags::SRGB_WRITE | VariantFlags::POINT_SPRITE,
        ..VariantKey::default()
    };
    // The stage encodes unless the pass's view does.
    assert_eq!(core_srgb_variant(base, false), base);
    assert_eq!(
        core_srgb_variant(base, true).flags,
        VariantFlags::POINT_SPRITE
    );
    let off = VariantKey::default();
    assert_eq!(core_srgb_variant(off, true), off);
}
