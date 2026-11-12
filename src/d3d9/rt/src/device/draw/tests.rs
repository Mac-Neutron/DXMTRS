use std::sync::Arc;

use mtld3d_core::{
    convert::{FanRewrite, ff_vs_layout_from_elements, fvf_to_elements},
    dxso::{MAX_LINKED_INPUTS, VsSamplerKinds},
    ff_state::FfState,
    ids::ProgramId,
    pipeline_key::{DeclCode, PipelineKey, PsStageKey, ShaderCode, VsStageKey},
};
use mtld3d_types::{
    D3DDECL_END, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_FLOAT2, D3DDECLTYPE_FLOAT4,
    D3DDECLUSAGE_POSITION, D3DDECLUSAGE_TEXCOORD, D3DFVF_DIFFUSE, D3DFVF_XYZRHW, D3DVERTEXELEMENT9,
    render_state_defaults,
};

use super::{Bind, DrawData, DrawRefused, Indices, Stage};
use crate::device::fixture::{at, core_on_fakes};

/// `vs_2_0 { dcl_position v0; mov oPos, v0 }`.
const VS_POS: [u32; 8] = [
    0xfffe_0200, // vs_2_0
    0x0200_001f,
    0x8000_0000,
    0x900f_0000, // dcl_position v0
    0x0200_0001,
    0xc00f_0000,
    0x90e4_0000, // mov oPos, v0
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

/// A `vs_2_0` version token and nothing else: no end token, so it does not parse.
const VS_TRUNCATED: [u32; 1] = [0xfffe_0200];

/// The declaration of `elements`, `D3DDECL_END` appended.
fn decl(elements: &[D3DVERTEXELEMENT9]) -> DeclCode {
    DeclCode::new(elements.iter().copied().chain([D3DDECL_END]).collect())
}

/// The program of `tokens`, in an `Arc` of its own.
fn code(tokens: &[u32]) -> ShaderCode {
    ShaderCode::from_parts(ProgramId::from_tokens(tokens), Arc::from(tokens))
}

/// One element of stream 0.
const fn element(offset: u16, type_: u8, usage: u8, usage_index: u8) -> D3DVERTEXELEMENT9 {
    D3DVERTEXELEMENT9 {
        stream: 0,
        offset,
        type_,
        method: D3DDECLMETHOD_DEFAULT,
        usage,
        usage_index,
    }
}

/// The key of a default-state `XYZRHW | DIFFUSE` draw, 20 bytes a vertex.
fn key(write_mask: u8) -> PipelineKey {
    let rs = render_state_defaults();
    let ff = FfState::new();
    let (elements, stride) = fvf_to_elements(D3DFVF_XYZRHW | D3DFVF_DIFFUSE);
    let layout = ff_vs_layout_from_elements(&elements);
    PipelineKey {
        vs: VsStageKey::Ff(ff.build_vs_key(&rs, layout, 0, [0; MAX_LINKED_INPUTS])),
        ps: PsStageKey::Ff(ff.build_ps_key(&rs, 0)),
        variant: ff.variant_key(&rs, true, false),
        layout: decl(&elements),
        stride,
        write_mask,
    }
}

/// The key of `vs` and `ps` over one POSITION0 FLOAT4, 16 bytes a vertex.
fn prog_key(vs: ShaderCode, ps: ShaderCode) -> PipelineKey {
    PipelineKey {
        vs: VsStageKey::Prog {
            code: vs,
            clip_plane_count: 0,
            sampler_kinds: VsSamplerKinds::default(),
        },
        ps: PsStageKey::Prog(ps),
        variant: mtld3d_core::dxso::VariantKey::default(),
        layout: decl(&[element(0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0)]),
        stride: 16,
        write_mask: 0xf,
    }
}

/// A full-target viewport of the fixture's 64x32 back buffer.
const VIEWPORT: [f64; 6] = [0.0, 0.0, 64.0, 32.0, 0.0, 1.0];

static VERTICES: [u8; 64] = [0; 64];

/// A draw of `primitive` over `vertex_count` of [`VERTICES`] with `binds`, unculled.
const fn draw<'a>(
    primitive: u8,
    vertex_count: u32,
    indices: Option<Indices<'a>>,
    binds: &'a [Bind<'a>],
) -> DrawData<'a> {
    DrawData {
        primitive,
        vertices: &VERTICES,
        vertex_count,
        indices,
        viewport: VIEWPORT,
        fill_mode: 0,
        cull_mode: 0,
        binds,
        textures: &[],
    }
}

/// A fixed-function triangle's binds, as C4a's arm makes them.
fn ff_binds() -> [Bind<'static>; 6] {
    static VS_CONSTANTS: [u8; 16] = [0; 16];
    static POS_FIXUP: [u8; 20] = [0; 20];
    static VS_DRAW: [u8; 32] = [0; 32];
    static PS_CONSTANTS: [u8; 16] = [0; 16];
    static ALPHA_REF: [u8; 4] = [0; 4];
    static FOG: [u8; 32] = [0; 32];
    let vs = |slot, bytes| Bind {
        stage: Stage::Vertex,
        slot,
        bytes,
    };
    let ps = |slot, bytes| Bind {
        stage: Stage::Pixel,
        slot,
        bytes,
    };
    [
        vs(30, &VS_CONSTANTS),
        vs(28, &POS_FIXUP),
        vs(27, &VS_DRAW),
        ps(15, &PS_CONSTANTS),
        ps(14, &ALPHA_REF),
        ps(13, &FOG),
    ]
}

/// A triangle's per-draw data: three 20-byte vertices.
fn triangle<'a>(binds: &'a [Bind<'a>]) -> DrawData<'a> {
    DrawData {
        vertices: &VERTICES[..60],
        ..draw(3, 3, None, binds)
    }
}

/// The pipelines `calls` shows made.
fn pipelines_made(calls: &[String]) -> usize {
    calls
        .iter()
        .filter(|c| *c == "MTLDevice_newRenderPipelineStateWithVertexDescriptor")
        .count()
}

/// The records of the first render pass in `calls`, which must load the cleared colour.
fn cleared_pass(calls: &[String]) -> &[String] {
    let encoder = at(calls, "MTLCommandBuffer_renderCommandEncoder");
    assert_eq!(
        calls[encoder + 1],
        "  color0 load=2 store=1 clear=0,0,1,1",
        "{calls:#?}"
    );
    let end = encoder
        + calls[encoder..]
            .iter()
            .position(|c| c == "MTLCommandEncoder_endEncoding")
            .expect("the pass ends");
    &calls[encoder..end]
}

/// The index in `pass` of the first record starting with `line`.
fn find(pass: &[String], line: &str) -> usize {
    pass.iter()
        .position(|c| c.starts_with(line))
        .unwrap_or_else(|| panic!("no {line:?} in the pass: {pass:#?}"))
}

/// Whether `calls` holds a draw record.
fn draws(calls: &[String]) -> bool {
    calls
        .iter()
        .any(|c| c.starts_with("  render 19 ") || c.starts_with("  render 20 "))
}

/// A fixed-function `DrawPrimitiveUP` reaches Metal in the cleared pass.
///
/// Its pipeline, the vertex bytes at buffer 0, the stages' constants at D3D9's slots, then one
/// draw, in the render pass the Clear before it became the load action of.
#[test]
fn ff_draw_up_records_pipeline_binds_and_draw() {
    const DEVICE: u64 = 0xb1;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0]);
    let binds = ff_binds();
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(
        &calls,
        "MTLDevice_newRenderPipelineStateWithVertexDescriptor",
    );
    let pass = cleared_pass(&calls);
    let pso = find(pass, "  render 15 ");
    let draw = find(
        pass,
        "  render 19 primitive_type=3 vertex_start=0 vertex_count=3 instance_count=1 \
         base_instance=0",
    );
    for line in [
        "  render 46 length=60 index=0",
        "  render 46 length=16 index=30",
        "  render 46 length=20 index=28",
        "  render 46 length=32 index=27",
        "  render 11 length=16 index=15",
    ] {
        let bind = find(pass, line);
        assert!(pso < bind && bind < draw, "{line}: {calls:#?}");
    }
}

/// One pipeline per key: the same key twice compiles once, another write mask compiles another.
#[test]
fn ff_pipeline_compiles_once_per_key() {
    const DEVICE: u64 = 0xb2;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
    assert_eq!(core.draw_up(&key(0x7), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
}

/// A pipeline that does not build drops its draws, and is not built again.
#[test]
fn ff_draw_with_a_failed_pipeline_records_nothing() {
    const DEVICE: u64 = dxmt_fakes::REFUSES_PIPELINES;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    let refused = Err(DrawRefused::Pipeline);
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), refused);
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), refused);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("  render 15 ")),
        "{calls:#?}"
    );
    assert!(!draws(&calls), "{calls:#?}");
}

/// A programmable pair's draw: pipeline, viewport, rasterizer state, vertices, binds, then draw.
#[test]
fn prog_draw_up_records_pipeline_binds_and_draw() {
    const DEVICE: u64 = 0xb3;
    static FLOATS: [u8; 64] = [0; 64];
    static INTS: [u8; 256] = [0; 256];
    static BOOLS: [u8; 4] = [0; 4];
    static POS_FIXUP: [u8; 20] = [0; 20];
    static PS_FLOATS: [u8; 16] = [0; 16];
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0]);
    let binds = [
        Bind {
            stage: Stage::Vertex,
            slot: 30,
            bytes: &FLOATS,
        },
        Bind {
            stage: Stage::Vertex,
            slot: 29,
            bytes: &INTS,
        },
        Bind {
            stage: Stage::Vertex,
            slot: 26,
            bytes: &BOOLS,
        },
        Bind {
            stage: Stage::Vertex,
            slot: 28,
            bytes: &POS_FIXUP,
        },
        Bind {
            stage: Stage::Pixel,
            slot: 15,
            bytes: &PS_FLOATS,
        },
    ];
    let culled = DrawData {
        cull_mode: 2,
        ..draw(4, 4, None, &binds)
    };
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&key, &culled), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let pass = cleared_pass(&calls);
    let pso = find(pass, "  render 15 ");
    let draw = find(
        pass,
        "  render 19 primitive_type=4 vertex_start=0 vertex_count=4",
    );
    for line in [
        "  render 13 viewports=48B viewport_count=1",
        "  render 12 fill_mode=0 cull_mode=2",
        "  render 46 length=64 index=0",
        "  render 46 length=64 index=30",
        "  render 46 length=256 index=29",
        "  render 46 length=4 index=26",
        "  render 46 length=20 index=28",
        "  render 11 length=16 index=15",
    ] {
        let at = find(pass, line);
        assert!(pso < at && at < draw, "{line}: {calls:#?}");
    }
}

/// A stage with nothing to bind at a slot binds nothing there.
#[test]
fn an_empty_bind_is_not_emitted() {
    const DEVICE: u64 = 0xb4;
    static ROW: [u8; 16] = [0; 16];
    let mut core = core_on_fakes(DEVICE);
    let binds = [
        Bind {
            stage: Stage::Vertex,
            slot: 30,
            bytes: &[],
        },
        Bind {
            stage: Stage::Pixel,
            slot: 15,
            bytes: &ROW,
        },
    ];
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&key, &draw(3, 3, None, &binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(&calls, "  render 11 length=16 index=15");
    assert!(
        !calls.iter().any(|c| c.contains(" length=0 ")),
        "{calls:#?}"
    );
}

/// An indexed UP draw takes its indices from the argument-buffer ring.
#[test]
fn indexed_draw_up_draws_from_the_ring() {
    const DEVICE: u64 = 0xb5;
    static INDICES: [u8; 12] = [0, 0, 1, 0, 2, 0, 2, 0, 1, 0, 3, 0];
    let mut core = core_on_fakes(DEVICE);
    let indices = Indices {
        bytes: &INDICES,
        index_type: 0,
        count: 6,
    };
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&key, &draw(3, 4, Some(indices), &[])), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let draw = calls
        .iter()
        .position(|c| {
            c.starts_with("  render 20 primitive_type=3 index_type=0 index_count=6 ")
                && c.ends_with(" base_vertex=0 base_instance=0")
        })
        .unwrap_or_else(|| panic!("no indexed draw: {calls:#?}"));
    let vertices = at(&calls, "  render 46 length=64 index=0");
    assert!(vertices < draw, "{calls:#?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("  render 19 ")),
        "{calls:#?}"
    );
}

/// A fan draws as the triangle list `FanRewrite` makes of it.
#[test]
fn a_fan_becomes_an_indexed_list() {
    const DEVICE: u64 = 0xb6;
    let mut core = core_on_fakes(DEVICE);
    let fan = FanRewrite::sequential(0, 2).expect("a fan of two triangles");
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    assert_eq!(list, [0, 0, 1, 0, 2, 0, 0, 0, 2, 0, 3, 0]);
    let indices = Indices {
        bytes: &list,
        index_type: fan.index_type() as u8,
        count: fan.index_count(),
    };
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&key, &draw(3, 4, Some(indices), &[])), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("  render 20 primitive_type=3 index_type=0 index_count=6 ")),
        "{calls:#?}"
    );
}

/// A vertex stage is compiled once whatever pixel stages it pairs with.
#[test]
fn a_vertex_shader_compiles_once_across_pixel_shaders() {
    const DEVICE: u64 = 0xb7;
    let mut core = core_on_fakes(DEVICE);
    let vs = code(&VS_POS);
    let first = prog_key(vs.clone(), code(&PS_C0));
    let second = prog_key(vs, code(&PS_C1));
    assert_eq!(core.draw_up(&first, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.draw_up(&second, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.pipelines_len(), 2);
    assert_eq!(core.compiles(), 3);
    assert_eq!(core.draw_up(&first, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.compiles(), 3);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 2, "{calls:#?}");
}

/// A program created again, its tokens in a new `Arc`, finds the pipeline the first one made.
#[test]
fn a_recreated_shader_hits_the_cache() {
    const DEVICE: u64 = 0xb8;
    let mut core = core_on_fakes(DEVICE);
    let first = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&first, &draw(3, 3, None, &[])), Ok(()));
    let compiles = core.compiles();
    let made = pipelines_made(&dxmt_fakes::take_calls(DEVICE));
    let again = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw_up(&again, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.compiles(), compiles);
    assert_eq!(core.pipelines_len(), 1);
    core.drain();
    assert_eq!(made, 1);
    assert_eq!(pipelines_made(&dxmt_fakes::take_calls(DEVICE)), 0);
}

/// A layout whose attribute crosses the stride is refused before anything compiles.
#[test]
fn a_crossing_draw_is_refused() {
    const DEVICE: u64 = 0xb9;
    let mut core = core_on_fakes(DEVICE);
    let key = PipelineKey {
        vs: VsStageKey::Prog {
            code: code(&[
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
            ]),
            clip_plane_count: 0,
            sampler_kinds: VsSamplerKinds::default(),
        },
        layout: decl(&[
            element(0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
            element(20, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        ]),
        ..prog_key(code(&VS_POS), code(&PS_C0))
    };
    let refused = Err(DrawRefused::Crossing);
    assert_eq!(core.draw_up(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.draw_up(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.compiles(), 0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 0, "{calls:#?}");
    assert!(!draws(&calls), "{calls:#?}");
}

/// A vertex stage that does not parse is kept as a failed entry: no re-parse, no draw.
#[test]
fn a_vertex_shader_that_does_not_parse_is_cached_as_failed() {
    const DEVICE: u64 = 0xba;
    let mut core = core_on_fakes(DEVICE);
    let key = prog_key(code(&VS_TRUNCATED), code(&PS_C0));
    let refused = Err(DrawRefused::Pipeline);
    assert_eq!(core.draw_up(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.draw_up(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.pipelines_len(), 1);
    assert_eq!(core.compiles(), 0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(!draws(&calls), "{calls:#?}");
}

/// A 2x2 BGRA8 texture `id` on `core`, uploaded.
fn texture(core: &mut crate::device::CoreDevice, id: u64) -> mtld3d_core::ids::TextureId {
    let id = mtld3d_core::ids::TextureId::from_raw(id);
    let desc = super::super::CoreTextureDesc {
        pixel_format: winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
        kind: super::super::TextureKind::Tex2d,
        width: 2,
        height: 2,
        depth: 1,
        levels: 1,
        view_twins: false,
        block: (1, 1, 4),
    };
    core.texture_create(id, &desc);
    let whole = super::super::UploadRegion {
        level: 0,
        slice: 0,
        x: 0,
        y: 0,
        z: 0,
        width: 2,
        height: 2,
        depth: 1,
    };
    assert!(core.texture_upload(id, whole, &[0u8; 16], 8, 16));
    id
}

/// A texture bound at the vertex stage is a vertex texture and sampler record.
///
/// One at vertex slot 2 beside one at pixel slot 0: one vertex texture, one vertex sampler and one
/// fragment texture, each before the draw.
#[test]
fn a_vertex_texture_binds_at_the_vertex_stage() {
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
        WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE, WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE,
    };
    const DEVICE: u64 = 0xe1;
    let mut core = core_on_fakes(DEVICE);
    let id = texture(&mut core, 1);
    let snapshot = mtld3d_core::sampler_state::snapshot_from_state(
        &mtld3d_types::sampler_state_defaults(),
        false,
    );
    let skey = mtld3d_core::sampler_state::key_from_snapshot(&snapshot);
    let sampler = core.sampler(
        skey,
        &mtld3d_core::sampler_state::description_from_snapshot(&snapshot, skey),
    );
    let vertex = core
        .texture_bind(
            Stage::Vertex,
            2,
            id,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            sampler.clone(),
        )
        .expect("a stored texture");
    let pixel = core
        .texture_bind(Stage::Pixel, 0, id, WMT_PIXEL_FORMAT_BGRA8_UNORM, sampler)
        .expect("a stored texture");
    let binds = ff_binds();
    let textures = [vertex, pixel];
    let data = DrawData {
        textures: &textures,
        ..triangle(&binds)
    };
    assert_eq!(core.draw_up(&key(0xf), &data), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let records = |prefix: String| -> Vec<usize> {
        calls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.starts_with(&prefix))
            .map(|(at, _)| at)
            .collect()
    };
    let draw_at = records("  render 19 ".to_owned());
    assert_eq!(draw_at.len(), 1, "{calls:#?}");
    let vertex_texture = records(format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE} "));
    let vertex_sampler = records(format!(
        "  render {WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE} "
    ));
    let fragment_texture = records(format!(
        "  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} "
    ));
    assert_eq!(vertex_texture.len(), 1, "{calls:#?}");
    assert_eq!(vertex_sampler.len(), 1, "{calls:#?}");
    assert_eq!(fragment_texture.len(), 1, "{calls:#?}");
    assert!(vertex_texture[0] < draw_at[0] && vertex_sampler[0] < draw_at[0]);
}

/// A textured draw binds its texture and sampler at the slot, after the PSO and before the draw.
#[test]
fn a_textured_draw_binds_texture_and_sampler() {
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
    };
    const DEVICE: u64 = 0xf1;
    let mut core = core_on_fakes(DEVICE);
    let id = texture(&mut core, 1);
    let snapshot = mtld3d_core::sampler_state::snapshot_from_state(
        &mtld3d_types::sampler_state_defaults(),
        false,
    );
    let skey = mtld3d_core::sampler_state::key_from_snapshot(&snapshot);
    let sampler = core.sampler(
        skey,
        &mtld3d_core::sampler_state::description_from_snapshot(&snapshot, skey),
    );
    let bind = core
        .texture_bind(Stage::Pixel, 0, id, WMT_PIXEL_FORMAT_BGRA8_UNORM, sampler)
        .expect("a stored texture");
    let binds = ff_binds();
    let textures = [bind];
    let data = DrawData {
        textures: &textures,
        ..triangle(&binds)
    };
    assert_eq!(core.draw_up(&key(0xf), &data), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let texture_at = calls
        .iter()
        .position(|c| {
            c.starts_with(&format!(
                "  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} "
            ))
        })
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let sampler_at = calls
        .iter()
        .position(|c| {
            c.starts_with(&format!(
                "  render {WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE} "
            ))
        })
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let draw_at = calls
        .iter()
        .position(|c| c.starts_with("  render 19 "))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(texture_at < draw_at && sampler_at < draw_at, "{calls:#?}");
}

/// Render, upload, render: one pass when unsampled, two when sampled.
///
/// When neither draw samples the uploaded texture the optimizer merges the two render passes
/// around the blit (map2-passes §3); when the first samples it, they stay two passes.
#[test]
fn render_blit_render_merges_when_unsampled() {
    const DEVICE: u64 = 0xf2;
    let render_passes = |calls: &[String]| {
        calls
            .iter()
            .filter(|c| *c == "MTLCommandBuffer_renderCommandEncoder")
            .count()
    };
    let mut core = core_on_fakes(DEVICE);
    let id = texture(&mut core, 2);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let binds = ff_binds();
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    let whole = super::super::UploadRegion {
        level: 0,
        slice: 0,
        x: 0,
        y: 0,
        z: 0,
        width: 2,
        height: 2,
        depth: 1,
    };
    assert!(core.texture_upload(id, whole, &[1u8; 16], 8, 16));
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let unsampled = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(render_passes(&unsampled), 1, "{unsampled:#?}");
    let bind = core
        .texture_bind(
            Stage::Pixel,
            0,
            id,
            winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
            None,
        )
        .expect("a stored texture");
    let textures = [bind];
    let sampled = DrawData {
        textures: &textures,
        ..triangle(&binds)
    };
    assert_eq!(core.draw_up(&key(0xf), &sampled), Ok(()));
    assert!(core.texture_upload(id, whole, &[2u8; 16], 8, 16));
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let split = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(render_passes(&split), 2, "{split:#?}");
}
