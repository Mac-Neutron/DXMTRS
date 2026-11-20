use std::sync::Arc;

use mtld3d_core::{
    convert::FanRewrite,
    dxso::VsSamplerKinds,
    ids::ProgramId,
    pipeline_key::{MAX_STREAMS, PipelineKey, PsStageKey, ShaderCode, StreamKey, VsStageKey},
    streams::STREAM_FREQ_DEFAULT,
};
use mtld3d_types::{
    D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_FLOAT2, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION,
    D3DDECLUSAGE_TEXCOORD, D3DSTREAMSOURCE_INSTANCEDATA, D3DVERTEXELEMENT9,
};

use super::{Bind, DrawData, DrawRefused, IndexData, Indices, Stage, StreamData, slot_bytes};
use crate::device::{
    CoreDevice,
    fixture::{
        at, bound, core_on_fakes, decl, descriptors, draw, ff_binds, inline, key, passes, stream0,
        triangle, two_stream_key,
    },
};

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
        streams: inline(16),
        write_mask: 0xf,
        color_format: winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
        depth_format: 0,
        blend: None,
        extras: [None, None, None],
        samples: 1,
        alpha_to_coverage: false,
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
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
    assert_eq!(core.draw(&key(0x7), &triangle(&binds)), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), refused);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), refused);
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
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
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
    assert_eq!(core.draw(&key, &culled), Ok(()));
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
    assert_eq!(core.draw(&key, &draw(3, 3, None, &binds)), Ok(()));
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
        data: IndexData::Bytes(&INDICES),
        index_type: 0,
        count: 6,
    };
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw(&key, &draw(3, 4, Some(indices), &[])), Ok(()));
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
        data: IndexData::Bytes(&list),
        index_type: fan.index_type() as u8,
        count: fan.index_count(),
    };
    let key = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw(&key, &draw(3, 4, Some(indices), &[])), Ok(()));
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
    assert_eq!(core.draw(&first, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.draw(&second, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.pipelines_len(), 2);
    assert_eq!(core.compiles(), 3);
    assert_eq!(core.draw(&first, &draw(3, 3, None, &[])), Ok(()));
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
    assert_eq!(core.draw(&first, &draw(3, 3, None, &[])), Ok(()));
    let compiles = core.compiles();
    let made = pipelines_made(&dxmt_fakes::take_calls(DEVICE));
    let again = prog_key(code(&VS_POS), code(&PS_C0));
    assert_eq!(core.draw(&again, &draw(3, 3, None, &[])), Ok(()));
    assert_eq!(core.compiles(), compiles);
    assert_eq!(core.pipelines_len(), 1);
    core.drain();
    assert_eq!(made, 1);
    assert_eq!(pipelines_made(&dxmt_fakes::take_calls(DEVICE)), 0);
}

/// A layout whose attribute crosses the stride binds the stream's bytes at a second slot.
///
/// The texture coordinates at offset 20 of 16-byte vertices: at slot 1, offset 0, under stream
/// 0's layout; the draw binds 64 bytes at slot 0 and 64 at slot 1. The capture prints a bind's
/// length alone: which bytes slot 1 holds is `an_inline_slot_reads_from_its_advance`'s to say.
#[test]
fn an_inline_crossing_draw_binds_its_bytes_at_two_slots() {
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
    assert_eq!(core.draw(&key, &draw(3, 3, None, &[])), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        descriptors(&calls),
        [[
            "  attr 0 format=31 offset=0 buffer=0",
            "  attr 3 format=29 offset=0 buffer=1",
            "  layout 0 stride=16 step=1 rate=1",
            "  layout 1 stride=16 step=1 rate=1",
        ]],
        "{calls:#?}"
    );
    let pass = passes(&calls)[0];
    find(pass, "  render 46 length=64 index=0");
    find(pass, "  render 46 length=64 index=1");
    assert!(draws(&calls), "{calls:#?}");
}

/// An inline slot's bytes are the stream's from its advance, zero-filled to the stream's length.
///
/// The stream itself at no advance; all zeros at an advance at or past its end.
#[test]
fn an_inline_slot_reads_from_its_advance() {
    let fed = [1_u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    assert_eq!(*slot_bytes(&fed, 0), fed);
    assert_eq!(
        *slot_bytes(&fed, 8),
        [9, 10, 11, 12, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(*slot_bytes(&fed, 12), [0; 12]);
    assert_eq!(*slot_bytes(&fed, 20), [0; 12]);
}

/// A vertex stage that does not parse is kept as a failed entry: no re-parse, no draw.
#[test]
fn a_vertex_shader_that_does_not_parse_is_cached_as_failed() {
    const DEVICE: u64 = 0xba;
    let mut core = core_on_fakes(DEVICE);
    let key = prog_key(code(&VS_TRUNCATED), code(&PS_C0));
    let refused = Err(DrawRefused::Pipeline);
    assert_eq!(core.draw(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.draw(&key, &draw(3, 3, None, &[])), refused);
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
        flags: super::super::CoreTextureFlags::empty(),
        srgb_format: 0,
        block: (1, 1, 4),
        samples: 1,
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
    assert_eq!(core.draw(&key(0xf), &data), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &data), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
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
    assert_eq!(core.draw(&key(0xf), &sampled), Ok(()));
    assert!(core.texture_upload(id, whole, &[2u8; 16], 8, 16));
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let split = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(render_passes(&split), 2, "{split:#?}");
}

/// A stored 64x64 one-level render-target texture of `pixel_format`, and its handle.
///
/// Its creation's calls (and everything before) are taken.
fn target_texture(
    core: &mut crate::device::CoreDevice,
    device: u64,
    id: u64,
    pixel_format: u32,
    texel_bytes: u32,
) -> (mtld3d_core::ids::TextureId, String) {
    use crate::device::{
        TextureKind,
        fixture::{target_desc, textures_made},
    };
    let id = mtld3d_core::ids::TextureId::from_raw(id);
    core.texture_create(
        id,
        &target_desc(pixel_format, TextureKind::Tex2d, 64, 1, texel_bytes),
    );
    core.drain();
    let made = textures_made(&dxmt_fakes::take_calls(device));
    (id, made.last().expect("a texture made").clone())
}

/// The whole of a 2D texture's level 0 as a target.
const fn level0(id: mtld3d_core::ids::TextureId) -> crate::device::TargetRef {
    crate::device::TargetRef::Texture {
        id,
        level: 0,
        face: None,
    }
}

/// A draw goes to the set target, with a pipeline of the key's colour format.
///
/// Two keys that differ only in the colour format are two pipelines.
#[test]
fn a_draw_targets_the_set_texture() {
    use winemetal_sys::wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_RGBA16_UNORM};

    use crate::device::fixture::targets;
    const DEVICE: u64 = 0xf9;
    let mut core = core_on_fakes(DEVICE);
    let (id, texture) = target_texture(&mut core, DEVICE, 1, WMT_PIXEL_FORMAT_RGBA16_UNORM, 8);
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert!(core.set_target(level0(id)));
    assert_eq!(core.target_format(), Some(WMT_PIXEL_FORMAT_RGBA16_UNORM));
    let wide = PipelineKey {
        color_format: WMT_PIXEL_FORMAT_RGBA16_UNORM,
        depth_format: 0,
        blend: None,
        ..key(0xf)
    };
    assert_eq!(core.draw(&wide, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 2, "{calls:#?}");
    let formats = calls
        .iter()
        .filter_map(|c| c.strip_prefix("  pso color0="))
        .collect::<Vec<_>>();
    assert_eq!(
        formats,
        [
            WMT_PIXEL_FORMAT_BGRA8_UNORM.to_string(),
            WMT_PIXEL_FORMAT_RGBA16_UNORM.to_string()
        ],
        "{calls:#?}"
    );
    let drawn = targets(&calls);
    assert_eq!(
        drawn.last(),
        Some(&(texture, "64x64".to_owned())),
        "{calls:#?}"
    );
    assert_eq!(drawn.len(), 2, "{calls:#?}");
}

/// A render-target texture made between two draws is zeroed ahead of them, outside their chunk.
///
/// The initializer's cleared pass on the new texture comes before the back buffer's one render
/// encoder, which holds both draws: nothing of the creation is recorded between them.
#[test]
fn a_target_made_between_two_draws_is_zeroed_ahead_of_them() {
    use winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM;

    use crate::device::{
        TextureKind,
        fixture::{target_desc, targets, textures_made},
    };
    const DEVICE: u64 = 0xfa;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.texture_create(
        mtld3d_core::ids::TextureId::from_raw(1),
        &target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 8, 1, 4),
    );
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = textures_made(&calls)[0].clone();
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [made, back_buffer], "{calls:#?}");
    let draws = calls
        .iter()
        .filter(|c| c.starts_with("  render 19 "))
        .count();
    assert_eq!(draws, 2, "{calls:#?}");
}

/// A target rewritten after it was sampled is not merged past its reader (C4a's ruling C4-3).
///
/// Draw to A, draw to the back buffer sampling A, draw to A, draw to the back buffer sampling A:
/// four passes in that order.
#[test]
fn a_rewritten_target_is_not_merged_past_its_reader() {
    use winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM;

    use crate::device::{
        TargetRef,
        fixture::{targets, textures_made},
    };
    const DEVICE: u64 = 0xfb;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let (id, texture) = target_texture(&mut core, DEVICE, 1, WMT_PIXEL_FORMAT_BGRA8_UNORM, 4);
    let binds = ff_binds();
    for _ in 0..2 {
        assert!(core.set_target(level0(id)));
        assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
        assert!(core.set_target(TargetRef::BackBuffer));
        let bind = core
            .texture_bind(Stage::Pixel, 0, id, WMT_PIXEL_FORMAT_BGRA8_UNORM, None)
            .expect("a stored texture");
        let textures = [bind];
        let sampling = DrawData {
            textures: &textures,
            ..triangle(&binds)
        };
        assert_eq!(core.draw(&key(0xf), &sampling), Ok(()));
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(
        handles,
        [texture.clone(), back_buffer.clone(), texture, back_buffer],
        "{calls:#?}"
    );
}

/// The fields of each scissor `calls` records, in order: `"<x> <y> <width> <height>"`.
fn scissors(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("  render 14 "))
        .map(|record| {
            ["x", "y", "width", "height"]
                .map(|name| {
                    record
                        .split_once(&format!("scissor_rects[0].{name}="))
                        .and_then(|(_, rest)| rest.split(' ').next())
                        .unwrap_or("?")
                        .to_owned()
                })
                .join(" ")
        })
        .collect()
}

/// A draw with `scissor` and the fixture's triangle.
fn scissored(core: &mut CoreDevice, scissor: [u32; 4]) -> Result<(), DrawRefused> {
    let binds = ff_binds();
    let draw = DrawData {
        scissor,
        ..triangle(&binds)
    };
    core.draw(&key(0xf), &draw)
}

/// A pass records its scissor at its first draw and not again for the same rect.
///
/// A new pass records it again: Metal keeps none across encoders, and a merged pass must not
/// inherit one.
#[test]
fn the_scissor_is_recorded_at_each_pass_start() {
    const DEVICE: u64 = 0x310;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(scissored(&mut core, [8, 4, 16, 8]), Ok(()));
    assert_eq!(scissored(&mut core, [8, 4, 16, 8]), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(scissors(&calls), ["8 4 16 8"], "{calls:#?}");
    assert_eq!(scissored(&mut core, [8, 4, 16, 8]), Ok(()));
    core.close_pass();
    assert_eq!(scissored(&mut core, [8, 4, 16, 8]), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(scissors(&calls), ["8 4 16 8", "8 4 16 8"], "{calls:#?}");
}

/// A changed rect is recorded, and the rect before it again when it comes back.
#[test]
fn a_scissor_change_is_recorded() {
    const DEVICE: u64 = 0x311;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    for rect in [[8, 4, 16, 8], [0, 0, 64, 32], [0, 0, 64, 32], [8, 4, 16, 8]] {
        assert_eq!(scissored(&mut core, rect), Ok(()));
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        scissors(&calls),
        ["8 4 16 8", "0 0 64 32", "8 4 16 8"],
        "{calls:#?}"
    );
}

/// A rect past the pass is cut to it: the 64x32 back buffer's.
#[test]
fn the_scissor_is_clamped_to_the_pass() {
    const DEVICE: u64 = 0x312;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(scissored(&mut core, [0, 0, 300, 300]), Ok(()));
    assert_eq!(scissored(&mut core, [60, 30, 300, 300]), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(scissors(&calls), ["0 0 64 32", "60 30 4 2"], "{calls:#?}");
}

/// A draw whose rect holds no pixel of the pass records nothing: no pass, no pipeline.
#[test]
fn an_empty_scissor_draws_nothing() {
    const DEVICE: u64 = 0x313;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    for rect in [[0, 0, 0, 8], [8, 8, 16, 0], [64, 0, 8, 8], [300, 300, 8, 8]] {
        assert_eq!(scissored(&mut core, rect), Ok(()));
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(passes(&calls).is_empty(), "{calls:#?}");
    assert_eq!(core.pipelines_len(), 0);
}

/// mtld3d's blend enums reach the wire by name: its numbers are not theirs.
#[test]
fn wire_blend_numbers() {
    use mtld3d_core::pipeline_state::{BlendFactor as F, BlendOperation as O};
    use winemetal_sys::wire as w;
    let factors = [
        (F::Zero, w::WMT_BLEND_FACTOR_ZERO),
        (F::One, w::WMT_BLEND_FACTOR_ONE),
        (F::SourceColor, w::WMT_BLEND_FACTOR_SOURCE_COLOR),
        (
            F::OneMinusSourceColor,
            w::WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
        ),
        (F::SourceAlpha, w::WMT_BLEND_FACTOR_SOURCE_ALPHA),
        (
            F::OneMinusSourceAlpha,
            w::WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        ),
        (F::DestinationAlpha, w::WMT_BLEND_FACTOR_DESTINATION_ALPHA),
        (
            F::OneMinusDestinationAlpha,
            w::WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        ),
        (F::DestinationColor, w::WMT_BLEND_FACTOR_DESTINATION_COLOR),
        (
            F::OneMinusDestinationColor,
            w::WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
        ),
        (
            F::SourceAlphaSaturated,
            w::WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
        ),
        (F::BlendColor, w::WMT_BLEND_FACTOR_BLEND_COLOR),
        (
            F::OneMinusBlendColor,
            w::WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
        ),
    ];
    for (factor, wire) in factors {
        assert_eq!(super::wire_factor(factor), wire, "{factor:?}");
    }
    // The pair a cast would swap.
    assert_eq!(super::wire_factor(F::DestinationColor), 6);
    assert_eq!(super::wire_factor(F::DestinationAlpha), 8);
    let ops = [
        (O::Add, w::WMT_BLEND_OPERATION_ADD),
        (O::Subtract, w::WMT_BLEND_OPERATION_SUBTRACT),
        (O::ReverseSubtract, w::WMT_BLEND_OPERATION_REVERSE_SUBTRACT),
        (O::Min, w::WMT_BLEND_OPERATION_MIN),
        (O::Max, w::WMT_BLEND_OPERATION_MAX),
    ];
    for (op, wire) in ops {
        assert_eq!(super::wire_op(op), wire, "{op:?}");
    }
}

/// Two keys that differ in their blend alone are two pipelines; the same blend again is none.
#[test]
fn a_blend_is_its_own_pipeline() {
    const DEVICE: u64 = 0x330;
    let mut core = core_on_fakes(DEVICE);
    let mut rs = mtld3d_types::render_state_defaults();
    rs[mtld3d_types::D3DRS_ALPHABLENDENABLE as usize] = 1;
    let blended = PipelineKey {
        blend: mtld3d_core::pipeline_state::core_blend(&rs, true),
        ..key(0xf)
    };
    assert!(blended.blend.is_some());
    let binds = ff_binds();
    for key in [&key(0xf), &blended, &blended, &key(0xf)] {
        assert_eq!(core.draw(key, &triangle(&binds)), Ok(()));
    }
    assert_eq!(core.pipelines_len(), 2);
}

/// The key entries of a UP draw at 16 bytes a vertex whose stream 1 is fed as `entry` says.
fn up_with(entry: StreamKey) -> [StreamKey; MAX_STREAMS as usize] {
    let mut streams = inline(16);
    streams[1] = entry;
    streams
}

/// A UP draw's declared stream with nothing bound reads zeros at its own slot.
///
/// Stream 0's attribute at buffer 0, stepping per vertex at the call's stride; stream 1's at
/// buffer 1, under a constant layout of the element's extent, and the draw binds that many zeros
/// at slot 1.
#[test]
fn a_declared_stream_with_nothing_bound_reads_zeros() {
    const DEVICE: u64 = 0x3c0;
    let mut core = core_on_fakes(DEVICE);
    let vertices = [0_u8; 48];
    let mut streams = stream0(&vertices);
    streams[1] = StreamData::Zeros;
    let binds = ff_binds();
    let drawn = DrawData {
        streams,
        ..draw(3, 3, None, &binds)
    };
    let key = two_stream_key(0, up_with(StreamKey::Null));
    assert_eq!(core.draw(&key, &drawn), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        descriptors(&calls),
        [[
            "  attr 0 format=31 offset=0 buffer=0",
            "  attr 2 format=42 offset=0 buffer=1",
            "  layout 0 stride=16 step=1 rate=1",
            "  layout 1 stride=4 step=0 rate=0",
        ]],
        "{calls:#?}"
    );
    let pass = passes(&calls)[0];
    find(pass, "  render 46 length=48 index=0");
    find(pass, "  render 46 length=4 index=1");
}

/// A bound stream's layout steps as its key entry says, and a zero stride is constant.
///
/// `INSTANCEDATA | 2` steps per instance at rate 2; a zero stride is a constant layout of the
/// element's extent under a per-vertex word and under a per-instance one.
#[test]
fn a_bound_entry_prints_its_step_and_rate() {
    const DEVICE: u64 = 0x3c1;
    let mut core = core_on_fakes(DEVICE);
    let (vertices, colors) = ([0_u8; 48], [0_u8; 12]);
    let mut streams = stream0(&vertices);
    streams[1] = StreamData::Bytes(&colors);
    let binds = ff_binds();
    let drawn = DrawData {
        streams,
        ..draw(3, 3, None, &binds)
    };
    let instanced = D3DSTREAMSOURCE_INSTANCEDATA | 2;
    for entry in [
        bound(4, instanced),
        bound(0, STREAM_FREQ_DEFAULT),
        bound(0, instanced),
    ] {
        assert_eq!(
            core.draw(&two_stream_key(0, up_with(entry)), &drawn),
            Ok(())
        );
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let layouts = descriptors(&calls)
        .iter()
        .map(|lines| {
            lines
                .iter()
                .rfind(|l| l.starts_with("  layout 1 "))
                .copied()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        layouts,
        [
            Some("  layout 1 stride=4 step=2 rate=2"),
            Some("  layout 1 stride=4 step=0 rate=0"),
            Some("  layout 1 stride=4 step=0 rate=0"),
        ],
        "{calls:#?}"
    );
}

/// Two keys that differ in one stream's stride alone are two pipelines, each at its stride.
#[test]
fn keys_of_two_strides_are_two_pipelines() {
    const DEVICE: u64 = 0x3c2;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    let wider = PipelineKey {
        streams: inline(24),
        ..key(0xf)
    };
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert_eq!(core.draw(&wider, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 2, "{calls:#?}");
    let strides = descriptors(&calls)
        .iter()
        .map(|lines| {
            lines
                .iter()
                .rfind(|l| l.starts_with("  layout 0 "))
                .copied()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        strides,
        [
            Some("  layout 0 stride=20 step=1 rate=1"),
            Some("  layout 0 stride=24 step=1 rate=1"),
        ],
        "{calls:#?}"
    );
}

/// An attribute wider than its stream's stride refuses every draw, and its pipeline never builds.
///
/// The colour's four bytes on stream 1, bound 2 bytes apart: no binding fetches them, and the key
/// is kept unbuilt.
#[test]
fn an_attribute_wider_than_its_stride_is_refused() {
    const DEVICE: u64 = 0x3c3;
    let mut core = core_on_fakes(DEVICE);
    let key = two_stream_key(0, up_with(bound(2, STREAM_FREQ_DEFAULT)));
    let refused = Err(DrawRefused::Fetch);
    assert_eq!(core.draw(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.draw(&key, &draw(3, 3, None, &[])), refused);
    assert_eq!(core.compiles(), 0);
    assert_eq!(core.pipelines_len(), 1);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 0, "{calls:#?}");
    assert!(!draws(&calls), "{calls:#?}");
}

/// Counts stored vertex-buffer binds in `calls`, excluding encoder-preamble dummy `#0` binds.
fn count_vb_binds(calls: &[String], suffix: &str) -> usize {
    let prefix = format!(
        "  render {} buffer=#",
        winemetal_sys::wire::WMT_RENDER_COMMAND_SET_VERTEX_BUFFER
    );
    calls
        .iter()
        .filter(|c| c.starts_with(&prefix) && !c.contains("buffer=#0 ") && c.ends_with(suffix))
        .count()
}

/// Counts records in `calls` whose line begins with `prefix`.
fn count_prefix(calls: &[String], prefix: &str) -> usize {
    calls.iter().filter(|c| c.starts_with(prefix)).count()
}

/// A `key` with stream 0 bound per vertex at `stride` bytes.
fn bound0_key(stride: u32) -> PipelineKey {
    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = bound(stride, STREAM_FREQ_DEFAULT);
    PipelineKey {
        streams,
        ..key(0xf)
    }
}

/// A stored buffer of `length` zeroed bytes, uploaded and drained.
fn stored_buffer(core: &mut CoreDevice, id: u64, length: usize) -> mtld3d_core::ids::BufferId {
    let id = mtld3d_core::ids::BufferId::from_raw(id);
    assert!(core.buffer_create(id, u64::try_from(length).expect("length fits u64")));
    assert!(core.buffer_upload(id, 0, &vec![0u8; length]));
    id
}

/// Draw streams with `id` bound at stream 0 at byte offset `offset`.
const fn stream0_buffer(
    id: mtld3d_core::ids::BufferId,
    offset: u32,
) -> [StreamData<'static>; MAX_STREAMS as usize] {
    let mut streams = [StreamData::None; MAX_STREAMS as usize];
    streams[0] = StreamData::Buffer { id, offset };
    streams
}

/// A default sampler on `core`.
fn default_sampler(core: &mut CoreDevice) -> Option<Arc<dxmt_core::sampler::Sampler>> {
    let snapshot = mtld3d_core::sampler_state::snapshot_from_state(
        &mtld3d_types::sampler_state_defaults(),
        false,
    );
    let skey = mtld3d_core::sampler_state::key_from_snapshot(&snapshot);
    core.sampler(
        skey,
        &mtld3d_core::sampler_state::description_from_snapshot(&snapshot, skey),
    )
}

/// A linear-filtered sampler on `core`, distinct from [`default_sampler`].
fn linear_sampler(core: &mut CoreDevice) -> Option<Arc<dxmt_core::sampler::Sampler>> {
    let mut states = mtld3d_types::sampler_state_defaults();
    states[mtld3d_types::D3DSAMP_MINFILTER as usize] = mtld3d_types::D3DTEXF_LINEAR;
    let snapshot = mtld3d_core::sampler_state::snapshot_from_state(&states, false);
    let skey = mtld3d_core::sampler_state::key_from_snapshot(&snapshot);
    core.sampler(
        skey,
        &mtld3d_core::sampler_state::description_from_snapshot(&snapshot, skey),
    )
}

/// Same-state draws in one pass deduplicate PSO, DSSO, blend, viewport, scissor, rasterizer, VB.
#[test]
fn same_state_draws_in_one_pass_deduplicate_all_state() {
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF, WMT_RENDER_COMMAND_SET_DSSO,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
        WMT_RENDER_COMMAND_SET_VERTEX_BYTES, WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
        WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE, WMT_RENDER_COMMAND_SET_VIEWPORTS,
    };
    const DEVICE: u64 = 0x501;
    let mut core = core_on_fakes(DEVICE);
    let vb = stored_buffer(&mut core, 1, 120);
    let tex_id = texture(&mut core, 2);
    let depth_id = mtld3d_core::ids::TextureId::from_raw(3);
    core.texture_create(
        depth_id,
        &crate::device::fixture::depth_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 32, 1),
    );
    assert!(core.set_depth(Some(crate::device::DepthRef {
        id: depth_id,
        level: 0
    })));
    let sampler = default_sampler(&mut core);
    let p_tex = core
        .texture_bind(
            Stage::Pixel,
            0,
            tex_id,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            sampler.clone(),
        )
        .expect("pixel texture");
    let v_tex = core
        .texture_bind(
            Stage::Vertex,
            1,
            tex_id,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            sampler,
        )
        .expect("vertex texture");
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let mut rs = mtld3d_types::render_state_defaults();
    rs[mtld3d_types::D3DRS_ALPHABLENDENABLE as usize] = 1;
    let pkey = PipelineKey {
        depth_format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        blend: mtld3d_core::pipeline_state::core_blend(&rs, true),
        ..bound0_key(20)
    };
    let binds = ff_binds();
    let textures = [p_tex, v_tex];
    let data = DrawData {
        streams: stream0_buffer(vb, 0),
        binds: &binds,
        textures: &textures,
        stencil_ref: 7,
        blend_color: Some([0.25, 0.5, 0.75, 1.0]),
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&pkey, &data), Ok(()));
    assert_eq!(core.draw(&pkey, &data), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls).len(), 1, "{calls:#?}");
    assert_eq!(count_prefix(&calls, "  render 19 "), 2, "{calls:#?}");
    assert_eq!(
        count_prefix(&calls, &format!("  render {WMT_RENDER_COMMAND_SET_PSO} ")),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(&calls, &format!("  render {WMT_RENDER_COMMAND_SET_DSSO} ")),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VIEWPORTS} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_SCISSOR_RECTS} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_RASTERIZER_STATE} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(count_vb_binds(&calls, ""), 1, "{calls:#?}");
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_BYTES} ")
        ),
        3,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES} ")
        ),
        3,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE} ")
        ),
        1,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE} ")
        ),
        1,
        "{calls:#?}"
    );
}

/// Merged render passes keep two full sets of state and binds; pass splits re-emit VB binds.
///
/// Covers `draw -> close_pass -> draw` merging via `dxmt-core` `optimize()`, plus toggling
/// `with_depth` and sRGB color format between draws using the same stored vertex buffer.
#[test]
fn merged_passes_and_pass_splits_reemit_full_state() {
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_RENDER_COMMAND_SET_PSO,
    };
    const DEVICE: u64 = 0x502;
    let mut core = core_on_fakes(DEVICE);
    let vb = stored_buffer(&mut core, 1, 120);
    let depth_id = mtld3d_core::ids::TextureId::from_raw(2);
    core.texture_create(
        depth_id,
        &crate::device::fixture::depth_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 32, 1),
    );
    assert!(core.set_depth(Some(crate::device::DepthRef {
        id: depth_id,
        level: 0
    })));
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let binds = ff_binds();
    let pkey = bound0_key(20);
    let data = DrawData {
        streams: stream0_buffer(vb, 0),
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&pkey, &data), Ok(()));
    core.close_pass();
    assert_eq!(core.draw(&pkey, &data), Ok(()));
    core.drain();
    let merged = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&merged).len(), 1, "{merged:#?}");
    assert_eq!(count_prefix(&merged, "  render 19 "), 2, "{merged:#?}");
    assert_eq!(
        count_prefix(&merged, &format!("  render {WMT_RENDER_COMMAND_SET_PSO} ")),
        2,
        "{merged:#?}"
    );
    assert_eq!(count_vb_binds(&merged, ""), 2, "{merged:#?}");

    // Toggling `with_depth` across draws re-emits `SET_VERTEX_BUFFER` on every pass.
    let with_depth_key = PipelineKey {
        depth_format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        ..pkey.clone()
    };
    assert_eq!(core.draw(&with_depth_key, &data), Ok(()));
    assert_eq!(core.draw(&pkey, &data), Ok(()));
    assert_eq!(core.draw(&with_depth_key, &data), Ok(()));
    core.drain();
    let depth_split = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&depth_split).len(), 3, "{depth_split:#?}");
    assert_eq!(count_vb_binds(&depth_split, ""), 3, "{depth_split:#?}");

    // Toggling sRGB color format across draws re-emits `SET_VERTEX_BUFFER` on every pass.
    let srgb_key = PipelineKey {
        color_format: WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        ..pkey.clone()
    };
    let linear_key = PipelineKey {
        color_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        ..pkey
    };
    assert_eq!(core.draw(&linear_key, &data), Ok(()));
    assert_eq!(core.draw(&srgb_key, &data), Ok(()));
    assert_eq!(core.draw(&linear_key, &data), Ok(()));
    core.drain();
    let srgb_split = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&srgb_split).len(), 3, "{srgb_split:#?}");
    assert_eq!(count_vb_binds(&srgb_split, ""), 3, "{srgb_split:#?}");
}

/// Helper passes (`clear_rects`, `stretch`, `depth_resolve`) between identical draws reset state.
#[test]
fn helper_passes_between_identical_draws_reemit_all_state() {
    use mtld3d_core::stretch_rect::BlitDecode;
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
        WMT_RENDER_COMMAND_SET_VIEWPORTS,
    };
    const DEVICE: u64 = 0x503;
    let mut core = core_on_fakes(DEVICE);
    let src_id = texture(&mut core, 1);
    let ms_depth_id = mtld3d_core::ids::TextureId::from_raw(2);
    core.texture_create(
        ms_depth_id,
        &crate::device::fixture::ms_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 4),
    );
    let ss_depth_id = mtld3d_core::ids::TextureId::from_raw(3);
    core.texture_create(
        ss_depth_id,
        &crate::device::fixture::depth_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 64, 1),
    );
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let binds = ff_binds();
    let pkey = key(0xf);
    let tri = triangle(&binds);

    let assert_counts =
        |calls: &[String], pso: usize, vp: usize, sc: usize, rast: usize, label: &str| {
            assert_eq!(
                count_prefix(calls, &format!("  render {WMT_RENDER_COMMAND_SET_PSO} ")),
                pso,
                "{label}: {calls:#?}"
            );
            assert_eq!(
                count_prefix(
                    calls,
                    &format!("  render {WMT_RENDER_COMMAND_SET_VIEWPORTS} ")
                ),
                vp,
                "{label}: {calls:#?}"
            );
            assert_eq!(
                count_prefix(
                    calls,
                    &format!("  render {WMT_RENDER_COMMAND_SET_SCISSOR_RECTS} ")
                ),
                sc,
                "{label}: {calls:#?}"
            );
            assert_eq!(
                count_prefix(
                    calls,
                    &format!("  render {WMT_RENDER_COMMAND_SET_RASTERIZER_STATE} ")
                ),
                rast,
                "{label}: {calls:#?}"
            );
        };

    // `clear_rects` between identical draws.
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    core.clear_rects(0, &[[0, 0, 8, 8]], [0.0, 1.0, 0.0, 1.0], false);
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    core.drain();
    let clear_calls = dxmt_fakes::take_calls(DEVICE);
    assert_counts(&clear_calls, 3, 2, 2, 2, "clear_rects");

    // `stretch` between identical draws.
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    assert!(core.stretch(&crate::device::StretchQuad {
        src: crate::device::TargetRef::Texture {
            id: src_id,
            level: 0,
            face: None,
        },
        src_rect: [0, 0, 2, 2],
        src_view: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        dst: crate::device::TargetRef::BackBuffer,
        dst_rect: [0, 0, 8, 8],
        linear: false,
        decode: BlitDecode::None.uniform(),
    }));
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    core.drain();
    let stretch_calls = dxmt_fakes::take_calls(DEVICE);
    assert_counts(&stretch_calls, 3, 3, 3, 3, "stretch");

    // `depth_resolve` between identical draws.
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    assert!(core.depth_resolve(
        crate::device::DepthRef {
            id: ms_depth_id,
            level: 0,
        },
        crate::device::DepthRef {
            id: ss_depth_id,
            level: 0,
        },
    ));
    assert_eq!(core.draw(&pkey, &tri), Ok(()));
    core.drain();
    let resolve_calls = dxmt_fakes::take_calls(DEVICE);
    assert_counts(&resolve_calls, 3, 3, 3, 3, "depth_resolve");
}

/// Slot cross-invalidation (`Buffer` <-> `Zeros` <-> `Buffer`), crossing fetch, and indexed paths.
#[test]
fn stream_slot_cross_invalidation_crossing_fetch_and_indexed_draws() {
    use winemetal_sys::wire::WMT_RENDER_COMMAND_SET_VERTEX_BYTES;
    const DEVICE: u64 = 0x504;
    let mut core = core_on_fakes(DEVICE);
    let vb0 = stored_buffer(&mut core, 1, 128);
    let vb1 = stored_buffer(&mut core, 2, 128);
    let ib = stored_buffer(&mut core, 3, 64);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    // Slot 1: `Buffer` -> `Zeros` -> `Buffer` in one pass must re-emit `SET_VERTEX_BUFFER` at slot 1.
    let key_buf1 = two_stream_key(0, up_with(bound(4, STREAM_FREQ_DEFAULT)));
    let key_null1 = two_stream_key(0, up_with(StreamKey::Null));
    let verts = [0_u8; 48];
    let mut s_buf = stream0(&verts);
    s_buf[1] = StreamData::Buffer { id: vb1, offset: 0 };
    let mut s_zero = stream0(&verts);
    s_zero[1] = StreamData::Zeros;
    let d_buf = DrawData {
        streams: s_buf,
        ..draw(3, 3, None, &[])
    };
    let d_zero = DrawData {
        streams: s_zero,
        ..draw(3, 3, None, &[])
    };
    assert_eq!(core.draw(&key_buf1, &d_buf), Ok(()));
    assert_eq!(core.draw(&key_null1, &d_zero), Ok(()));
    assert_eq!(core.draw(&key_buf1, &d_buf), Ok(()));
    core.drain();
    let cross_calls = dxmt_fakes::take_calls(DEVICE);
    let slot1_bufs = count_vb_binds(&cross_calls, " index=1");
    let slot1_bytes = cross_calls
        .iter()
        .filter(|c| {
            c.starts_with(&format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_BYTES} "))
                && c.ends_with(" index=1")
        })
        .count();
    assert_eq!(slot1_bufs, 2, "{cross_calls:#?}");
    assert_eq!(slot1_bytes, 1, "{cross_calls:#?}");

    // Crossing fetch (`advance != 0`) for stored `Buffer` dedups both slots on repeat draw.
    let mut cross_streams = [StreamKey::Unused; MAX_STREAMS as usize];
    cross_streams[0] = bound(16, STREAM_FREQ_DEFAULT);
    let cross_buf_key = PipelineKey {
        vs: VsStageKey::Prog {
            code: code(&[
                0xfffe_0200,
                0x0200_001f,
                0x8000_0000,
                0x900f_0000,
                0x0200_001f,
                0x8001_0005,
                0x900f_0003,
                0x0200_0001,
                0xc00f_0000,
                0x90e4_0000,
                0x0200_0001,
                0xe00f_0000,
                0x90e4_0003,
                0x0000_ffff,
            ]),
            clip_plane_count: 0,
            sampler_kinds: VsSamplerKinds::default(),
        },
        layout: decl(&[
            element(0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITION, 0),
            element(20, D3DDECLTYPE_FLOAT2, D3DDECLUSAGE_TEXCOORD, 1),
        ]),
        streams: cross_streams,
        ..prog_key(code(&VS_POS), code(&PS_C0))
    };
    let d_cross_buf = DrawData {
        streams: stream0_buffer(vb0, 0),
        ..draw(3, 3, None, &[])
    };
    assert_eq!(core.draw(&cross_buf_key, &d_cross_buf), Ok(()));
    assert_eq!(core.draw(&cross_buf_key, &d_cross_buf), Ok(()));
    core.drain();
    let cross_buf_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_vb_binds(&cross_buf_calls, ""),
        2,
        "{cross_buf_calls:#?}"
    );
    assert_eq!(
        count_prefix(&cross_buf_calls, "  render 19 "),
        2,
        "{cross_buf_calls:#?}"
    );

    // Crossing fetch (`advance != 0`) for inline `Bytes` emits both slots on every draw.
    let cross_up_key = PipelineKey {
        streams: inline(16),
        ..cross_buf_key
    };
    let d_cross_up = draw(3, 3, None, &[]);
    assert_eq!(core.draw(&cross_up_key, &d_cross_up), Ok(()));
    assert_eq!(core.draw(&cross_up_key, &d_cross_up), Ok(()));
    core.drain();
    let cross_up_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_prefix(
            &cross_up_calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_BYTES} ")
        ),
        4,
        "{cross_up_calls:#?}"
    );

    // Stored `IndexData::Buffer` draws twice in one pass deduping VB while emitting 2 indexed draws.
    let ib_key = bound0_key(20);
    let d_ib = DrawData {
        streams: stream0_buffer(vb0, 0),
        indices: Some(Indices {
            data: IndexData::Buffer { id: ib, offset: 0 },
            index_type: 0,
            count: 3,
        }),
        ..draw(3, 3, None, &[])
    };
    assert_eq!(core.draw(&ib_key, &d_ib), Ok(()));
    assert_eq!(core.draw(&ib_key, &d_ib), Ok(()));
    core.drain();
    let ib_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(count_vb_binds(&ib_calls, ""), 1, "{ib_calls:#?}");
    assert_eq!(count_prefix(&ib_calls, "  render 20 "), 2, "{ib_calls:#?}");

    // Inline `IndexData::Bytes` draws twice without hitting non-indexed fast path.
    let raw_idx = [0_u8, 0, 1, 0, 2, 0];
    let d_idx_up = DrawData {
        indices: Some(Indices {
            data: IndexData::Bytes(&raw_idx),
            index_type: 0,
            count: 3,
        }),
        ..triangle(&[])
    };
    let up_key = key(0xf);
    assert_eq!(core.draw(&up_key, &d_idx_up), Ok(()));
    assert_eq!(core.draw(&up_key, &d_idx_up), Ok(()));
    core.drain();
    let idx_up_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_prefix(&idx_up_calls, "  render 20 "),
        2,
        "{idx_up_calls:#?}"
    );
    assert_eq!(
        count_prefix(&idx_up_calls, "  render 19 "),
        0,
        "{idx_up_calls:#?}"
    );
}

/// Constant byte mutation (same length different bytes, different length) on vertex and pixel.
#[test]
fn constant_byte_mutation_reemits_only_changed_slots() {
    use winemetal_sys::wire::{
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_VERTEX_BYTES,
    };
    const DEVICE: u64 = 0x505;
    let mut core = core_on_fakes(DEVICE);
    let vb = stored_buffer(&mut core, 1, 120);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let pkey = bound0_key(20);
    let mut draw_consts = |vs: &[u8], ps: &[u8]| {
        let binds = [
            Bind {
                stage: Stage::Vertex,
                slot: 30,
                bytes: vs,
            },
            Bind {
                stage: Stage::Pixel,
                slot: 15,
                bytes: ps,
            },
        ];
        let data = DrawData {
            streams: stream0_buffer(vb, 0),
            ..draw(3, 3, None, &binds)
        };
        assert_eq!(core.draw(&pkey, &data), Ok(()));
    };

    draw_consts(&[1_u8; 16], &[2_u8; 16]);
    draw_consts(&[1_u8; 16], &[2_u8; 16]);
    draw_consts(&[3_u8; 16], &[4_u8; 16]);
    draw_consts(&[3_u8; 32], &[4_u8; 32]);
    draw_consts(&[3_u8; 32], &[4_u8; 32]);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls).len(), 1, "{calls:#?}");
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_BYTES} ")
        ),
        3,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES} ")
        ),
        3,
        "{calls:#?}"
    );
}

/// Independent pixel/vertex texture and sampler swaps plus self-sampling tile barrier.
#[test]
fn independent_texture_sampler_changes_and_self_sampling_tile_barrier() {
    use winemetal_sys::wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
        WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
        WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE,
    };
    const DEVICE: u64 = 0x506;
    let mut core = core_on_fakes(DEVICE);
    let vb = stored_buffer(&mut core, 1, 120);
    let tex_a = mtld3d_core::ids::TextureId::from_raw(10);
    let tex_b = mtld3d_core::ids::TextureId::from_raw(11);
    let rt_desc = crate::device::fixture::twinned_ms_desc(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        16,
        1,
    );
    core.texture_create(tex_a, &rt_desc);
    core.texture_create(tex_b, &rt_desc);
    let samp_a = default_sampler(&mut core);
    let samp_b = linear_sampler(&mut core);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let pkey = bound0_key(20);
    let draw_tex = |core: &mut CoreDevice, binds: &[crate::device::TextureBind]| {
        let data = DrawData {
            streams: stream0_buffer(vb, 0),
            textures: binds,
            ..draw(3, 3, None, &[])
        };
        assert_eq!(core.draw(&pkey, &data), Ok(()));
    };

    // Pixel stage: sampler swap leaves texture deduped; view/texture swap leaves sampler deduped.
    let px_a_samp_a = core
        .texture_bind(
            Stage::Pixel,
            0,
            tex_a,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            samp_a.clone(),
        )
        .expect("tex_a");
    let px_a_samp_b = core
        .texture_bind(
            Stage::Pixel,
            0,
            tex_a,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            samp_b.clone(),
        )
        .expect("tex_a");
    let px_a_plain = core
        .texture_bind(Stage::Pixel, 0, tex_a, WMT_PIXEL_FORMAT_BGRA8_UNORM, None)
        .expect("tex_a");
    let px_a_srgb = core
        .texture_bind(
            Stage::Pixel,
            0,
            tex_a,
            WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
            None,
        )
        .expect("tex_a srgb");
    let px_other_plain = core
        .texture_bind(Stage::Pixel, 0, tex_b, WMT_PIXEL_FORMAT_BGRA8_UNORM, None)
        .expect("tex_b");

    draw_tex(&mut core, &[px_a_samp_a]);
    draw_tex(&mut core, &[px_a_samp_b]);
    draw_tex(&mut core, &[px_a_plain]);
    draw_tex(&mut core, &[px_a_srgb]);
    draw_tex(&mut core, &[px_other_plain]);
    core.drain();
    let px_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_prefix(
            &px_calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} ")
        ),
        3,
        "{px_calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &px_calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE} ")
        ),
        3,
        "{px_calls:#?}"
    );

    // Vertex stage independence.
    let vx_a_samp_a = core
        .texture_bind(
            Stage::Vertex,
            0,
            tex_a,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            samp_a,
        )
        .expect("v tex_a");
    let vx_a_samp_b = core
        .texture_bind(
            Stage::Vertex,
            0,
            tex_a,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            samp_b.clone(),
        )
        .expect("v tex_a");
    let vx_other_samp = core
        .texture_bind(
            Stage::Vertex,
            0,
            tex_b,
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            samp_b,
        )
        .expect("v tex_b");
    draw_tex(&mut core, &[vx_a_samp_a]);
    draw_tex(&mut core, &[vx_a_samp_b]);
    draw_tex(&mut core, &[vx_other_samp]);
    core.drain();
    let vx_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_prefix(
            &vx_calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE} ")
        ),
        2,
        "{vx_calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &vx_calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE} ")
        ),
        2,
        "{vx_calls:#?}"
    );

    // Self-sampling tile barrier (`render 41`) with deduped PSO recovery on Draw 2.
    assert!(core.set_target(crate::device::TargetRef::Texture {
        id: tex_a,
        level: 0,
        face: None,
    }));
    draw_tex(&mut core, &[]);
    let self_bind = core
        .texture_bind(Stage::Pixel, 0, tex_a, WMT_PIXEL_FORMAT_BGRA8_UNORM, None)
        .expect("self bind");
    draw_tex(&mut core, &[self_bind]);
    core.drain();
    let tb_calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&tb_calls).len(), 1, "{tb_calls:#?}");
    let barrier_idx = find(
        &tb_calls,
        &format!("  render {WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE}"),
    );
    let pso_prefix = format!("  render {WMT_RENDER_COMMAND_SET_PSO} ");
    let first_pso = tb_calls
        .iter()
        .find(|c| c.starts_with(&pso_prefix))
        .expect("initial PSO");
    let recovered_pso = tb_calls[barrier_idx + 1..]
        .iter()
        .find(|c| c.starts_with(&pso_prefix))
        .expect("recovered PSO");
    assert_eq!(first_pso, recovered_pso, "{tb_calls:#?}");
}

/// Occlusion queries (`bump_visibility_result_offset`) fire on all compact fast-path closures.
#[test]
fn occlusion_queries_bump_visibility_on_all_fast_path_closures() {
    use winemetal_sys::wire::WMT_RENDER_COMMAND_SET_VISIBILITY_MODE;
    const DEVICE: u64 = 0x507;
    let mut core = core_on_fakes(DEVICE);
    let vb = stored_buffer(&mut core, 1, 120);
    let ib = stored_buffer(&mut core, 2, 64);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let vis_prefix = format!("  render {WMT_RENDER_COMMAND_SET_VISIBILITY_MODE}");
    let mut q = crate::device::Occlusion::default();

    // Closure A: non-indexed stored VB.
    let k_stored = bound0_key(20);
    let d_stored = DrawData {
        streams: stream0_buffer(vb, 0),
        ..draw(3, 3, None, &[])
    };
    assert_eq!(core.draw(&k_stored, &d_stored), Ok(()));
    core.occlusion_begin(&mut q);
    assert_eq!(core.draw(&k_stored, &d_stored), Ok(()));
    core.occlusion_end(&mut q);
    assert_eq!(core.draw(&k_stored, &d_stored), Ok(()));
    core.drain();
    let calls_a = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls_a).len(), 1, "{calls_a:#?}");
    assert_eq!(count_prefix(&calls_a, &vis_prefix), 2, "{calls_a:#?}");

    // Closure B: stored indexed.
    let d_idx = DrawData {
        streams: stream0_buffer(vb, 0),
        indices: Some(Indices {
            data: IndexData::Buffer { id: ib, offset: 0 },
            index_type: 0,
            count: 3,
        }),
        ..draw(3, 3, None, &[])
    };
    assert_eq!(core.draw(&k_stored, &d_idx), Ok(()));
    core.occlusion_begin(&mut q);
    assert_eq!(core.draw(&k_stored, &d_idx), Ok(()));
    core.occlusion_end(&mut q);
    assert_eq!(core.draw(&k_stored, &d_idx), Ok(()));
    core.drain();
    let calls_b = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls_b).len(), 1, "{calls_b:#?}");
    assert_eq!(count_prefix(&calls_b, &vis_prefix), 3, "{calls_b:#?}");

    // Closure C: single inline UP vertex draw.
    let k_up = key(0xf);
    let d_up = triangle(&[]);
    assert_eq!(core.draw(&k_up, &d_up), Ok(()));
    core.occlusion_begin(&mut q);
    assert_eq!(core.draw(&k_up, &d_up), Ok(()));
    core.occlusion_end(&mut q);
    assert_eq!(core.draw(&k_up, &d_up), Ok(()));
    core.drain();
    let calls_c = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls_c).len(), 1, "{calls_c:#?}");
    assert_eq!(count_prefix(&calls_c, &vis_prefix), 3, "{calls_c:#?}");
}

/// `PsoMemo` O(1) `key_gen` hit, MRU promotion, `key_gen == 0` bypass, >24 eviction, and refusals.
#[test]
fn pso_memo_key_gen_mru_eviction_and_refusal_exclusion() {
    use winemetal_sys::wire::WMT_RENDER_COMMAND_SET_PSO;
    const DEVICE: u64 = 0x508;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    let d_gen = |key_gen: u64| DrawData {
        key_gen,
        ..triangle(&binds)
    };

    // Index-0 `key_gen` hit + `last_draw_gen` fast-path.
    let k_a = key(0xf);
    let k_b = key(0xe);
    assert_eq!(core.draw(&k_a, &d_gen(10)), Ok(()));
    assert_eq!(core.draw(&k_a, &d_gen(10)), Ok(()));
    assert_eq!(core.pipelines_len(), 1);
    assert_eq!(core.pso_memo.len(), 1);

    // Miss index 0 with `k_b` (`key_gen: 20`), then return to `k_a` with new `key_gen: 30`
    // (MRU hit promotes `k_a` back to index 0 and refreshes `key_gen` to 30), then hit 30.
    assert_eq!(core.draw(&k_b, &d_gen(20)), Ok(()));
    assert_eq!(core.draw(&k_a, &d_gen(30)), Ok(()));
    assert_eq!(core.draw(&k_a, &d_gen(30)), Ok(()));
    assert_eq!(core.pipelines_len(), 2);
    assert_eq!(core.pso_memo.len(), 2);

    // `key_gen == 0` bypasses index-0 `key_gen` match and matches by `(key_hash, key)`.
    assert_eq!(core.draw(&k_b, &d_gen(0)), Ok(()));
    assert_eq!(core.draw(&k_b, &d_gen(0)), Ok(()));
    assert_eq!(core.pipelines_len(), 2);

    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        count_prefix(&calls, &format!("  render {WMT_RENDER_COMMAND_SET_PSO} ")),
        4,
        "{calls:#?}"
    );
    assert_eq!(pipelines_made(&calls), 2, "{calls:#?}");

    // >24 unique keys evicts oldest from `PsoMemo` while `PsoMap` still holds them.
    let ps = code(&PS_C0);
    let mut keys_26 = Vec::new();
    for i in 0..26_u16 {
        let mut s = [StreamKey::Unused; MAX_STREAMS as usize];
        s[0] = StreamKey::Inline {
            stride: 16 + u32::from(i) * 4,
        };
        keys_26.push(PipelineKey {
            streams: s,
            ..prog_key(code(&VS_POS), ps.clone())
        });
    }
    let base_len = core.pso_memo.len();
    for (idx, k) in keys_26.iter().enumerate() {
        assert_eq!(core.draw(k, &draw(3, 3, None, &[])), Ok(()));
        assert_eq!(core.pso_memo.len(), (base_len + idx + 1).min(24));
    }
    assert_eq!(core.pso_memo.len(), 24);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    // Drawing the evicted first key hits `PsoMap` without compiling a new pipeline.
    assert_eq!(core.draw(&keys_26[0], &draw(3, 3, None, &[])), Ok(()));
    core.drain();
    let after_evict = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&after_evict), 0, "{after_evict:#?}");

    // Refused layout (`DrawRefused::Fetch`) is never cached in `PsoMemo`.
    let mut bad_core = core_on_fakes(0x509);
    let refused_key = two_stream_key(0, up_with(bound(2, STREAM_FREQ_DEFAULT)));
    assert_eq!(
        bad_core.draw(&refused_key, &draw(3, 3, None, &[])),
        Err(DrawRefused::Fetch)
    );
    assert_eq!(bad_core.pso_memo.len(), 0);

    // Failed pipeline (`DrawRefused::Pipeline`) is never cached in `PsoMemo`.
    let mut fail_core = core_on_fakes(dxmt_fakes::REFUSES_PIPELINES);
    assert_eq!(
        fail_core.draw(&key(0xf), &triangle(&binds)),
        Err(DrawRefused::Pipeline)
    );
    assert_eq!(fail_core.pso_memo.len(), 0);
}

/// `target_gen` bumps on every target/depth/back-buffer mutator and stays unchanged on no-ops.
#[test]
fn target_gen_bumps_on_every_target_mutator() {
    use winemetal_sys::wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT};
    const DEVICE: u64 = 0x50a;
    let mut core = core_on_fakes(DEVICE);
    let mut g = core.target_gen();

    let (rt1, _) = target_texture(&mut core, DEVICE, 1, WMT_PIXEL_FORMAT_BGRA8_UNORM, 4);
    let (rt2, _) = target_texture(&mut core, DEVICE, 2, WMT_PIXEL_FORMAT_BGRA8_UNORM, 4);
    let rt2_desc = crate::device::fixture::target_desc(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        crate::device::TextureKind::Tex2d,
        64,
        1,
        4,
    );
    let depth_id = mtld3d_core::ids::TextureId::from_raw(3);
    core.texture_create(
        depth_id,
        &crate::device::fixture::depth_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 64, 1),
    );
    let unbound_id = texture(&mut core, 4);

    // Unrelated texture creation did not bump `target_gen`.
    assert_eq!(core.target_gen(), g);

    // `set_target` bumps on change, no-op on same.
    assert!(core.set_target(level0(rt1)));
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();
    assert!(core.set_target(level0(rt1)));
    assert_eq!(core.target_gen(), g);

    // `set_extra_target` bumps on change, no-op on same.
    assert!(core.set_extra_target(1, Some(level0(rt2))));
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();
    assert!(core.set_extra_target(1, Some(level0(rt2))));
    assert_eq!(core.target_gen(), g);

    // `set_depth` bumps on change, no-op on same.
    let dref = crate::device::DepthRef {
        id: depth_id,
        level: 0,
    };
    assert!(core.set_depth(Some(dref)));
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();
    assert!(core.set_depth(Some(dref)));
    assert_eq!(core.target_gen(), g);

    // `texture_create` replacing a bound extra/target/depth bumps `target_gen`.
    core.texture_create(rt2, &rt2_desc);
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();

    // `forget_texture` on an unbound texture does not bump `target_gen`.
    core.forget_texture(unbound_id);
    assert_eq!(core.target_gen(), g);

    // `forget_texture` on bound target/extra/depth bumps `target_gen`.
    core.forget_texture(rt1);
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();
    core.forget_texture(rt2);
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();
    core.forget_texture(depth_id);
    assert_ne!(core.target_gen(), g);
    g = core.target_gen();

    // `resize` (`make_back_buffer`) bumps `target_gen`.
    core.resize(128, 64, 1);
    assert_ne!(core.target_gen(), g);
}

/// A `key_gen == 0` draw resets `last_draw_gen` so returning to `K` re-emits state A.
#[test]
fn key_gen_zero_resets_last_draw_gen_in_open_pass() {
    use winemetal_sys::wire::{WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VIEWPORTS};
    const DEVICE: u64 = 0x50b;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    let k_a = key(0xf);
    let k_b = key(0x7);
    let vp_a = [0.0, 0.0, 64.0, 32.0, 0.0, 1.0];
    let vp_b = [0.0, 0.0, 32.0, 16.0, 0.0, 1.0];
    let d_a = DrawData {
        viewport: vp_a,
        key_gen: 10,
        ..triangle(&binds)
    };
    let d_b = DrawData {
        viewport: vp_b,
        key_gen: 0,
        ..triangle(&binds)
    };

    assert_eq!(core.draw(&k_a, &d_a), Ok(()));
    assert_eq!(core.draw(&k_b, &d_b), Ok(()));
    assert_eq!(core.draw(&k_a, &d_a), Ok(()));
    core.drain();

    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls).len(), 1, "{calls:#?}");
    assert_eq!(count_prefix(&calls, "  render 19 "), 3, "{calls:#?}");
    assert_eq!(
        count_prefix(&calls, &format!("  render {WMT_RENDER_COMMAND_SET_PSO} ")),
        3,
        "{calls:#?}"
    );
    assert_eq!(
        count_prefix(
            &calls,
            &format!("  render {WMT_RENDER_COMMAND_SET_VIEWPORTS} ")
        ),
        3,
        "{calls:#?}"
    );
}
