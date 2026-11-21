use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
    ptr::NonNull,
    sync::Arc,
};

use airconv_ir::dxso::spec::{StageSpec, VsSamplerKinds, VsSpec};
use mtld3d_shared::mtl::{PixelFormat, VertexFormat, VertexStepFunction};
use mtld3d_types::{
    D3DDECL_END, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_D3DCOLOR, D3DDECLTYPE_FLOAT2,
    D3DDECLTYPE_FLOAT3, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_COLOR, D3DDECLUSAGE_NORMAL,
    D3DDECLUSAGE_POSITION, D3DDECLUSAGE_POSITIONT, D3DDECLUSAGE_TEXCOORD, D3DFVF_DIFFUSE,
    D3DFVF_SPECULAR, D3DFVF_XYZRHW, D3DSTREAMSOURCE_INDEXEDDATA, D3DSTREAMSOURCE_INSTANCEDATA,
    D3DVERTEXELEMENT9, D3DVIEWPORT9, render_state_defaults,
};
use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::Xxh3;

use super::{
    CoreExtra, CoreRefusal, DeclCode, DrawFacts, DrawFlags, MAX_STREAMS, PipelineKey,
    PipelineSource, PsStageKey, SampledMasks, ShaderCode, StageKind, StreamKey, VsStageKey,
    core_coverage_variant, core_draw_kinds, core_extras, core_key_depth, core_ps_variant,
    core_scissor, core_srgb_variant, core_viewport, extra_present, float_rows, indexed_up_vertices,
    pipeline_source, pos_fixup, rhw_depth_clamp, up_byte_count, viewport_depth_range,
};
use crate::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements, resolve_attrs_for_ff, rhw_passthrough},
    draw_data::{AttrSnapshot, DeclarationHeader, StreamBinding},
    dxso::{
        FF_PASSTHROUGH_ATTR_BASE, FfVsKey, VariantFlags, VariantKey, ff_attr_index_for_semantic,
    },
    encoder_draw::draw_record::{StreamRecord, VertexBytes, VertexView, stream_layouts_view},
    ff_state::FfState,
    ids::{BufferId, ProgramId},
    multisample::SAMPLE_MASK_ALL,
    pipeline_state::StreamLayout,
    streams::{STREAM_FREQ_DEFAULT, VertexFetchError, stream_step},
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

/// The streams of a UP draw at `stride` over a declaration of stream 0 alone.
fn inline(stride: u32) -> [StreamKey; MAX_STREAMS as usize] {
    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = StreamKey::Inline { stride };
    streams
}

/// The key entry of a bound stream: its stride and the step and rate of its frequency word.
const fn bound(stride: u32, freq: u32) -> StreamKey {
    let (step, rate) = stream_step(freq);
    StreamKey::Bound {
        stride,
        step,
        rate,
        shift: 0,
    }
}

/// The key entry of a per-vertex stream bound at an offset whose low two bits are `shift`.
const fn shifted(stride: u32, shift: u8) -> StreamKey {
    let (step, rate) = stream_step(STREAM_FREQ_DEFAULT);
    StreamKey::Bound {
        stride,
        step,
        rate,
        shift,
    }
}

/// Each Metal slot bound to its own stream, unadvanced: the bindings of a source with no fetch.
fn own_slots() -> [(u8, u32); MAX_STREAMS as usize] {
    std::array::from_fn(|slot| (u8::try_from(slot).expect("a slot below 16"), 0))
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
        streams: inline(stride),
        write_mask: 0xf,
        color_format: PixelFormat::Bgra8Unorm as u32,
        depth_format: 0,
        blend: None,
        extras: [None, None, None],
        samples: 1,
        alpha_to_coverage: false,
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
        streams: inline(28),
        write_mask: 0xf,
        color_format: PixelFormat::Bgra8Unorm as u32,
        depth_format: 0,
        blend: None,
        extras: [None, None, None],
        samples: 1,
        alpha_to_coverage: false,
    }
}

/// [`prog_base`] from a buffer on stream 0, 28 bytes a vertex, its offset's low bits `shift`.
fn prog_bound(shift: u8) -> PipelineKey {
    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = shifted(28, shift);
    PipelineKey {
        streams,
        ..prog_base()
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
    let changes: [Case; 22] = [
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
        ("streams", ff_base, |k| k.streams = inline(24)),
        // A shift alone: the same stride, step and rate.
        (
            "streams (shift)",
            || prog_bound(0),
            |k| k.streams[0] = shifted(28, 1),
        ),
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
        // On the fixed-function stage: a programmable one exports exactly the key's extras.
        ("extras", ff_base, |k| {
            k.extras[1] = Some(extra(PixelFormat::Rgba16Float as u32));
        }),
        ("samples", ff_base, |k| k.samples = 4),
        ("alpha_to_coverage", ff_base, |k| k.alpha_to_coverage = true),
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
            (0, VertexFormat::Float4 as u32, 0, 0),
            (3, VertexFormat::Float2 as u32, 16, 0)
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
    // Stream 0 steps by the call's 28 bytes; the attributes the stage reads end at 24.
    assert_eq!(
        source.layouts[0],
        StreamLayout {
            stride: 28,
            step: VertexStepFunction::PerVertex,
            step_rate: 1,
        }
    );
    assert!(source.layouts[1..].iter().all(|l| !l.is_used()));
    assert_eq!(source.crossing, 0);
    assert_eq!(source.ps.spec, StageSpec::Ps(VariantKey::default()));
    // Nothing crosses and nothing is shifted: no fetch, each stream at its own slot.
    assert_eq!(source.fetch, Ok(()));
    assert_eq!(source.streams, source.layouts);
    assert_eq!(source.bindings, own_slots());
}

/// A UP draw's element on a stream with nothing bound is provided, at its stream's slot.
///
/// To both stages: the stream reads zeros through a constant layout of its extent.
#[test]
fn elements_off_stream_0_are_provided_at_their_stream() {
    let mut streams = inline(16);
    streams[1] = StreamKey::Null;
    let prog = PipelineKey {
        layout: decl(&[
            element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
            element(1, 0, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        ]),
        streams,
        ..prog_base()
    };
    let prog = source(&prog);
    assert_eq!(provided(&prog), 0b1001);
    assert_eq!(
        prog.attrs,
        [
            (0, VertexFormat::Float4 as u32, 0, 0),
            (3, VertexFormat::Float2 as u32, 0, 1)
        ]
    );
    assert_eq!(
        prog.layouts[1],
        StreamLayout {
            stride: 8,
            step: VertexStepFunction::Constant,
            step_rate: 0,
        }
    );

    let ff_mask = |elements: &[D3DVERTEXELEMENT9]| {
        let key = PipelineKey {
            vs: VsStageKey::Ff(ff_vs(elements)),
            layout: decl(elements),
            streams,
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
    assert_ne!(on_1 & color0, 0, "and from stream 1");
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
        streams: inline(28),
        ..ff_base()
    };
    let source = source(&key);
    let passthrough = u32::from(FF_PASSTHROUGH_ATTR_BASE);
    assert!(
        source
            .attrs
            .contains(&(passthrough, VertexFormat::Float3 as u32, 16, 0)),
        "{:?}",
        source.attrs
    );
    assert_ne!(provided(&source) & (1 << passthrough), 0);
}

/// An attribute ending past its stream's stride crosses into the next element, on any stream.
#[test]
fn a_crossing_attribute_is_detected() {
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
        element(0, 20, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
    ];
    let crossing = PipelineKey {
        layout: decl(&elements),
        streams: inline(16),
        ..prog_base()
    };
    assert_eq!(source(&crossing).crossing, 0b01);
    let whole = PipelineKey {
        streams: inline(28),
        ..crossing
    };
    assert_eq!(source(&whole).crossing, 0);
    assert_eq!(source(&prog_base()).crossing, 0);
    // The texture coordinates on stream 1, bound 4 bytes apart under an 8-byte element.
    let mut streams = inline(16);
    streams[1] = bound(4, STREAM_FREQ_DEFAULT);
    let on_1 = PipelineKey {
        layout: decl(&[
            element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
            element(1, 0, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        ]),
        streams,
        ..prog_base()
    };
    assert_eq!(source(&on_1).crossing, 0b10);
}

/// A crossing attribute is fetched at a spare slot, unshifted and on a shifted stream.
///
/// At offset 0 of the slot (plus the stream's shift), under its stream's layout, through a
/// binding of its stream advanced by its offset; the streams keep their own layouts.
#[test]
fn a_crossing_attribute_is_fetched_at_a_spare_slot() {
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
        element(0, 20, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
    ];
    for shift in [0, 2] {
        let mut key = PipelineKey {
            layout: decl(&elements),
            ..prog_bound(0)
        };
        key.streams[0] = shifted(16, shift);
        let source = source(&key);
        assert_eq!(source.crossing, 0b01, "{shift}");
        assert_eq!(source.fetch, Ok(()), "{shift}");
        let moved = u32::from(shift);
        assert_eq!(
            source.attrs,
            [
                (0, VertexFormat::Float4 as u32, moved, 0),
                (3, VertexFormat::Float2 as u32, moved, 1)
            ],
            "{shift}"
        );
        assert_eq!(source.layouts[..2], [source.streams[0]; 2], "{shift}");
        assert!(!source.streams[1].is_used(), "{shift}");
        assert_eq!(source.bindings[..2], [(0, 0), (0, 20)], "{shift}");
        assert_eq!(source.bindings[2..], own_slots()[2..], "{shift}");
    }
}

/// A stream bound off four bytes moves the attributes it feeds by its shift, at their own slots.
///
/// On stream 0, and on either of two streams alone: the other stream's attributes stay.
#[test]
fn a_shifted_stream_moves_the_attributes_it_feeds() {
    let on_0 = source(&prog_bound(2));
    assert_eq!(on_0.fetch, Ok(()));
    assert_eq!(
        on_0.attrs,
        [
            (0, VertexFormat::Float4 as u32, 2, 0),
            (3, VertexFormat::Float2 as u32, 18, 0)
        ]
    );
    assert_eq!(on_0.layouts, on_0.streams);
    assert_eq!(on_0.bindings, own_slots());
    // The position on stream 0, 16 bytes a vertex; the texture coordinates on stream 1, 8.
    let two = |shift_0: u8, shift_1: u8| {
        let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
        streams[0] = shifted(16, shift_0);
        streams[1] = shifted(8, shift_1);
        source(&PipelineKey {
            layout: decl(&[
                element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
                element(1, 0, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
            ]),
            streams,
            ..prog_base()
        })
    };
    let on_1 = two(0, 3);
    assert_eq!(
        on_1.attrs,
        [
            (0, VertexFormat::Float4 as u32, 0, 0),
            (3, VertexFormat::Float2 as u32, 3, 1)
        ]
    );
    assert_eq!(on_1.bindings, own_slots());
    let on_0_of_2 = two(1, 0);
    assert_eq!(
        on_0_of_2.attrs,
        [
            (0, VertexFormat::Float4 as u32, 1, 0),
            (3, VertexFormat::Float2 as u32, 0, 1)
        ]
    );
    // A shift on a stream the stage does not read moves nothing and fetches nothing.
    let mut unread = prog_bound(0);
    let mut elements = prog_elements().to_vec();
    elements.push(element(1, 0, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0));
    unread.layout = decl(&elements);
    unread.streams[1] = shifted(4, 3);
    let unread = source(&unread);
    assert!(!unread.streams[1].is_used());
    assert_eq!(unread.attrs, source(&prog_bound(0)).attrs);
    assert_eq!(unread.bindings, own_slots());
}

/// An attribute wider than its stream's stride cannot be fetched by any binding.
///
/// The source says why, and keeps each stream at its own slot with its attributes unmoved.
#[test]
fn an_attribute_wider_than_its_stride_cannot_be_fetched() {
    let mut key = prog_bound(0);
    // The FLOAT4 position is 16 bytes under an 8-byte stride.
    key.streams[0] = shifted(8, 0);
    let source = source(&key);
    assert_eq!(source.crossing, 0b01);
    assert_eq!(
        source.fetch,
        Err(VertexFetchError::AttributeWiderThanStride)
    );
    assert!(!source.fetchable());
    assert_eq!(
        source.attrs,
        [
            (0, VertexFormat::Float4 as u32, 0, 0),
            (3, VertexFormat::Float2 as u32, 16, 0)
        ]
    );
    assert_eq!(source.layouts, source.streams);
    assert_eq!(source.bindings, own_slots());
}

/// The core's layouts are mtld3d's for every way a stream is fed.
///
/// Stream by stream against `stream_layouts_view`, the closure mtld3d's draws run, over the same
/// declaration and bindings: inline, bound, a zero stride under each kind of word, per instance,
/// an instance rate of 0, nothing bound, and strides short of their extent. The crossing streams
/// too.
#[test]
fn layouts_are_mtld3ds_for_every_feed() {
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT3, D3DDECLUSAGE_POSITION, 0),
        element(1, 0, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0),
        element(2, 0, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 0),
    ];
    let vs = ff_vs(&elements);
    let resolved = resolve_attrs_for_ff(&elements, &vs.passthrough);
    assert_eq!(resolved.used_streams, 0b111);
    assert_eq!(resolved.extents[..3], [12, 4, 8]);
    let header = DeclarationHeader {
        vdecl_hash: 0,
        extents: resolved.extents,
        count: u32::try_from(resolved.attrs.len()).expect("three attributes"),
        used_streams: resolved.used_streams,
        reserved: 0,
    };
    // SAFETY: the attributes and the header outlive the token, which is only read below.
    let attrs =
        unsafe { AttrSnapshot::new(NonNull::from(&resolved.attrs[0]), NonNull::from(&header)) };
    let base = PipelineKey {
        vs: VsStageKey::Ff(vs),
        layout: decl(&elements),
        ..ff_base()
    };
    let mtld3d = |view: &VertexView<'_>| {
        let mut layouts = [StreamLayout::UNUSED; MAX_STREAMS as usize];
        let (mut crossing, mut offsets) = (0, 0);
        stream_layouts_view(&mut layouts, view, &attrs, &mut crossing, &mut offsets);
        (layouts, crossing)
    };
    let core = |streams| {
        let source = source(&PipelineKey {
            streams,
            ..base.clone()
        });
        (source.streams, source.crossing)
    };

    // A UP draw 16 bytes a vertex: stream 0 inline, the other two nothing bound.
    let bytes = VertexBytes {
        address: 0,
        length: 0,
        size: 0,
    };
    let up = VertexView::Up {
        record: &bytes,
        stride: 16,
    };
    let mut streams = inline(16);
    streams[1] = StreamKey::Null;
    streams[2] = StreamKey::Null;
    assert_eq!(core(streams), mtld3d(&up), "inline");

    let (indexed, instanced) = (D3DSTREAMSOURCE_INDEXEDDATA, D3DSTREAMSOURCE_INSTANCEDATA);
    let default = Some((12, STREAM_FREQ_DEFAULT));
    // Each stream's binding, `(stride, frequency word)`, none for nothing bound; the crossing
    // streams.
    let cases = [
        (
            "per vertex",
            [default, Some((4, indexed | 3)), Some((8, 1))],
            0,
        ),
        (
            "zero strides",
            [
                Some((0, indexed | 3)),
                Some((0, instanced | 2)),
                Some((0, 1)),
            ],
            0,
        ),
        (
            "zero stride, rate 0",
            [default, Some((0, instanced)), None],
            0,
        ),
        (
            "per instance",
            [default, Some((4, instanced | 2)), Some((8, instanced | 1))],
            0,
        ),
        (
            "instance rate 0",
            [default, Some((4, instanced)), Some((8, instanced))],
            0,
        ),
        ("nothing bound", [default, None, None], 0),
        (
            "short strides",
            [Some((8, 1)), Some((4, 1)), Some((4, instanced | 2))],
            0b101,
        ),
    ];
    for (name, bindings, crossing) in cases {
        let records = (0_u8..)
            .zip(bindings)
            .filter_map(|(stream, binding)| {
                binding.map(|(stride, freq)| {
                    StreamRecord::from_binding(&StreamBinding {
                        stream,
                        buffer_id: BufferId::from_raw(1),
                        backing_ptr: 0,
                        backing_len: 64,
                        backing_generation: 0,
                        offset: 0,
                        stride,
                        freq,
                    })
                })
            })
            .collect::<Vec<_>>();
        let view = VertexView::Bound {
            records: &records,
            stream0_freq: STREAM_FREQ_DEFAULT,
        };
        let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
        for (key, binding) in streams.iter_mut().zip(bindings) {
            *key = binding.map_or(StreamKey::Null, |(stride, freq)| bound(stride, freq));
        }
        let expected = mtld3d(&view);
        assert_eq!(expected.1, crossing, "{name}");
        assert_eq!(core(streams), expected, "{name}");
    }
}

/// Core's pixel variant: no samplers, one sample, its linked inputs and the pass's extras.
///
/// A programmable stage exports colour 0 and each extra the pass attaches (bit `i + 1` for slot
/// `i + 1`), and keeps its depth output exactly when a depth-stencil is attached; the
/// fixed-function stage keeps the base's outputs.
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
    let prog = core_ps_variant(base, StageKind::Prog, false, 0b101, false, 0, false);
    assert_eq!(prog.color_out_mask, 0);
    assert_eq!(prog.linked_input_mask, 0b101);
    assert_eq!(prog.alpha_func, 5);
    assert_eq!(prog.sample_mask, 0);
    assert_eq!(
        prog.flags,
        VariantFlags::SRGB_WRITE | VariantFlags::NO_DEPTH_ATTACHMENT
    );
    let vpos = core_ps_variant(base, StageKind::Prog, false, 0, true, 0, true);
    assert_eq!(
        vpos.flags,
        VariantFlags::SRGB_WRITE | VariantFlags::VPOS_SCALE
    );
    // (extras the pass attaches, bit i slot i + 1) -> the programmable stage's outputs.
    for (extras, outputs) in [(0b101, 0b1010), (0b010, 0b0100), (0b111, 0b1110), (0, 0)] {
        let prog = core_ps_variant(base, StageKind::Prog, false, 0, false, extras, false);
        assert_eq!(prog.color_out_mask, outputs, "{extras:#b}");
    }
    let points = core_ps_variant(base, StageKind::Prog, true, 0, false, 0, false);
    assert!(points.flags.contains(VariantFlags::POINT_SPRITE));
    let ff = core_ps_variant(base, StageKind::Ff, false, 0, false, 0b101, true);
    assert_eq!(ff.flags, VariantFlags::SRGB_WRITE);
    assert_eq!(ff.linked_input_mask, 0);
    assert_eq!(ff.color_out_mask, 0b110);
    // With a depth-stencil attached the stage exports its depth, even from a base that had not.
    let no_depth_base = VariantKey {
        flags: base.flags | VariantFlags::NO_DEPTH_ATTACHMENT,
        ..base
    };
    let with_depth = core_ps_variant(no_depth_base, StageKind::Prog, false, 0, true, 0, false);
    assert_eq!(with_depth.flags, VariantFlags::SRGB_WRITE);
    assert_eq!(
        core_ps_variant(base, StageKind::Ff, false, 0, true, 0, false).flags,
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
/// otherwise none for a draw that uses no plane, and a refusal for one that does. One of another
/// sample count than target 0 is none before anything else, never a refusal.
#[test]
fn core_key_depth_covers_every_case() {
    const F: u32 = 260;
    // (bound, sampled by the draw, uses a plane, target 0's count) -> the key's format.
    let cases = [
        (None, false, true, true, Ok(0)),
        (None, false, false, true, Ok(0)),
        (None, true, true, true, Ok(0)),
        (Some(F), false, true, true, Ok(F)),
        (Some(F), false, false, true, Ok(F)),
        (Some(F), true, false, true, Ok(0)),
        (
            Some(F),
            true,
            true,
            true,
            Err(CoreRefusal::BoundDepthSampled),
        ),
        (Some(F), false, true, false, Ok(0)),
        (Some(F), true, true, false, Ok(0)),
        (None, false, true, false, Ok(0)),
    ];
    for (bound, sampled, uses_plane, same_samples, expected) in cases {
        assert_eq!(
            core_key_depth(bound, sampled, uses_plane, same_samples),
            expected,
            "{bound:?} {sampled} {uses_plane} {same_samples}"
        );
    }
}

/// The coverage a draw adds to its pixel variant: the narrowed sample mask, alpha-to-coverage.
///
/// The all-samples mask leaves the variant alone; any other sets the flag and the byte. Coverage
/// from alpha zeroes the alpha test's function and leaves the mask as it was.
#[test]
fn core_coverage_variant_rules() {
    let base = VariantKey {
        alpha_func: 5,
        ..VariantKey::default()
    };
    assert_eq!(core_coverage_variant(base, SAMPLE_MASK_ALL, false), base);
    let masked = core_coverage_variant(base, 0x3, false);
    assert!(masked.flags.contains(VariantFlags::SAMPLE_MASK));
    assert_eq!((masked.sample_mask, masked.alpha_func), (0x3, 5));
    let a2c = core_coverage_variant(base, SAMPLE_MASK_ALL, true);
    assert_eq!(a2c.alpha_func, 0);
    assert!(!a2c.flags.contains(VariantFlags::SAMPLE_MASK));
    assert_eq!(a2c.sample_mask, base.sample_mask);
    let both = core_coverage_variant(base, 0x1, true);
    assert_eq!((both.sample_mask, both.alpha_func), (0x1, 0));
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
        bits(pos_fixup(&viewport(128, 64), target, false, 0.75, 0.0)),
        bits([1.0 / 128.0, -1.0 / 64.0, 0.0, 0.75, 0.0])
    );
    assert_eq!(
        bits(pos_fixup(&viewport(0, 64), target, true, 1.0, -0.25)),
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

/// An extra of `format`, every channel written, unblended.
const fn extra(format: u32) -> CoreExtra {
    CoreExtra {
        format,
        write_mask: 0xf,
        blend: None,
    }
}

/// The key's colour formats: colour 0's, then each extra's, 0 for an empty slot.
#[test]
fn color_formats_lists_the_slots() {
    const RGBA8: u32 = PixelFormat::Rgba8Unorm as u32;
    const R32F: u32 = PixelFormat::R32Float as u32;
    let bgra8 = PixelFormat::Bgra8Unorm as u32;
    assert_eq!(ff_base().color_formats(), [bgra8, 0, 0, 0]);
    let key = PipelineKey {
        extras: [Some(extra(RGBA8)), None, Some(extra(R32F))],
        ..ff_base()
    };
    assert_eq!(key.color_formats(), [bgra8, RGBA8, 0, R32F]);
}

/// Bit `i` for each slot `i + 1` a pass attaches.
#[test]
fn extra_present_bits() {
    assert_eq!(extra_present([0, 0, 0]), 0);
    assert_eq!(extra_present([80, 0, 0]), 0b001);
    assert_eq!(extra_present([0, 70, 0]), 0b010);
    assert_eq!(extra_present([0, 0, 55]), 0b100);
    assert_eq!(extra_present([80, 0, 115]), 0b101);
    assert_eq!(extra_present([80, 70, 55]), 0b111);
}

/// Each extra takes its own slot's format, write mask and destination-alpha clamp, as mtld3d's.
///
/// Slot `n`'s mask is `D3DRS_COLORWRITEENABLE<n>` when the program writes `oC<n>`, empty when it
/// does not (the target is declared and keeps its contents); its blend is the draw's.
#[test]
fn core_extras_follow_mtld3d() {
    use mtld3d_types::{
        D3DBLEND_DESTALPHA, D3DBLEND_INVDESTALPHA, D3DRS_ALPHABLENDENABLE, D3DRS_COLORWRITEENABLE1,
        D3DRS_COLORWRITEENABLE2, D3DRS_COLORWRITEENABLE3, D3DRS_DESTBLEND, D3DRS_SRCBLEND,
    };

    use crate::pipeline_state::{BlendFactor, core_blend};
    const F1: u32 = PixelFormat::Rgba8Unorm as u32;
    const F2: u32 = PixelFormat::Rgba16Float as u32;
    const F3: u32 = PixelFormat::R32Float as u32;
    let mut rs = render_state_defaults();
    // Red, green, blue: Metal's 8, 4 and 2.
    rs[D3DRS_COLORWRITEENABLE1 as usize] = 0x1;
    rs[D3DRS_COLORWRITEENABLE2 as usize] = 0x2;
    rs[D3DRS_COLORWRITEENABLE3 as usize] = 0x4;
    let every = 0b1111;
    let mask = |extra: &Option<CoreExtra>| extra.as_ref().map(|e| e.write_mask);
    // A slot not in the pass is none, whatever the states and the masks.
    assert_eq!(
        core_extras([0, 0, 0], &rs, every, every),
        [None, None, None]
    );
    // Slot 2 alone: COLORWRITEENABLE2, and bit 2 of the written mask, not bit 1.
    let [one, two, three] = core_extras([0, F2, 0], &rs, 0b0100, 0);
    assert_eq!((one, three), (None, None));
    assert_eq!(
        two,
        Some(CoreExtra {
            format: F2,
            write_mask: 4,
            blend: None,
        })
    );
    assert_eq!(mask(&core_extras([0, F2, 0], &rs, 0b0010, 0)[1]), Some(0));
    // Slot 3 takes COLORWRITEENABLE3.
    assert_eq!(mask(&core_extras([0, 0, F3], &rs, 0b1000, 0)[2]), Some(2));
    // A present slot the program does not write (the fixed-function stage writes colour 0
    // alone): an empty mask, its format kept.
    assert_eq!(
        core_extras([F1, 0, 0], &rs, 1, 0)[0],
        Some(CoreExtra {
            format: F1,
            write_mask: 0,
            blend: None,
        })
    );
    // Blending on: the draw's equation, each slot clamped on its own alpha.
    rs[D3DRS_ALPHABLENDENABLE as usize] = 1;
    rs[D3DRS_SRCBLEND as usize] = D3DBLEND_DESTALPHA;
    rs[D3DRS_DESTBLEND as usize] = D3DBLEND_INVDESTALPHA;
    let [one, two, three] = core_extras([F1, F2, 0], &rs, every, 0b0100);
    assert_eq!(three, None);
    let one = one.and_then(|e| e.blend).expect("slot 1 blends");
    let two = two.and_then(|e| e.blend).expect("slot 2 blends");
    assert_eq!((one.src, one.dst), (BlendFactor::One, BlendFactor::Zero));
    assert_eq!(
        (two.src, two.dst),
        (
            BlendFactor::DestinationAlpha,
            BlendFactor::OneMinusDestinationAlpha
        )
    );
    let zero = core_blend(&rs, true).expect("colour 0 blends");
    assert_eq!(two, zero);
    assert_eq!(
        (one.op, one.op_alpha, two.op, two.op_alpha),
        (zero.op, zero.op_alpha, zero.op, zero.op_alpha)
    );
}

#[test]
fn elements_with_end_preserves_terminator_verbatim() {
    let custom_end = D3DVERTEXELEMENT9 {
        stream: 0xff,
        offset: 0,
        type_: mtld3d_types::D3DDECLTYPE_UNUSED,
        method: 0,
        usage: 0,
        usage_index: 7,
    };
    let pos = D3DVERTEXELEMENT9 {
        stream: 0,
        offset: 0,
        type_: D3DDECLTYPE_FLOAT4,
        method: D3DDECLMETHOD_DEFAULT,
        usage: D3DDECLUSAGE_POSITION,
        usage_index: 0,
    };
    let code = DeclCode::new(Arc::from([pos, custom_end]));
    assert_eq!(code.elements(), &[pos]);
    assert_eq!(code.elements_with_end(), &[pos, custom_end]);
}
