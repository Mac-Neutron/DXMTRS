use mtld3d_core::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements},
    ff_pipeline::FfPipelineKey,
    ff_state::FfState,
};
use mtld3d_types::{D3DFVF_DIFFUSE, D3DFVF_XYZRHW, render_state_defaults};

use super::FfDrawData;
use crate::device::fixture::{at, core_on_fakes};

/// The key of a default-state `XYZRHW | DIFFUSE` draw, 20 bytes a vertex.
fn key(write_mask: u8) -> FfPipelineKey {
    let rs = render_state_defaults();
    let ff = FfState::new();
    let fvf = D3DFVF_XYZRHW | D3DFVF_DIFFUSE;
    let layout = ff_vs_layout_from_elements(&fvf_to_elements(fvf).0);
    FfPipelineKey {
        vs: ff.build_vs_key(&rs, layout, 0, [0; 8]),
        ps: ff.build_ps_key(&rs, 0),
        variant: ff.variant_key(&rs, true, false),
        fvf,
        stride: 20,
        write_mask,
    }
}

/// A triangle's per-draw data: three 20-byte vertices.
fn triangle() -> FfDrawData<'static> {
    static VERTICES: [u8; 60] = [0; 60];
    static VS_CONSTANTS: [u8; 16] = [0; 16];
    static VS_DRAW: [u8; 32] = [0; 32];
    static PS_CONSTANTS: [u8; 16] = [0; 16];
    static ALPHA_REF: [u8; 4] = [0; 4];
    static FOG: [u8; 32] = [0; 32];
    FfDrawData {
        primitive: 3,
        vertex_count: 3,
        vertices: &VERTICES,
        vs_constants: &VS_CONSTANTS,
        pos_fixup: [1.0 / 64.0, -1.0 / 32.0, 1.0, 1.0, 0.0],
        vs_draw: &VS_DRAW,
        ps_constants: &PS_CONSTANTS,
        alpha_ref: &ALPHA_REF,
        fog: &FOG,
    }
}

/// The pipelines `calls` shows made.
fn pipelines_made(calls: &[String]) -> usize {
    calls
        .iter()
        .filter(|c| *c == "MTLDevice_newRenderPipelineStateWithVertexDescriptor")
        .count()
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
    assert_eq!(core.draw_up(&key(0xf), &triangle()), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(
        &calls,
        "MTLDevice_newRenderPipelineStateWithVertexDescriptor",
    );
    let encoder = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
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
    let pass = &calls[encoder..end];
    let find = |line: &str| {
        pass.iter()
            .position(|c| c.starts_with(line))
            .unwrap_or_else(|| panic!("no {line:?} in the cleared pass: {calls:#?}"))
    };
    let pso = find("  render 15 ");
    let vertices = find("  render 46 length=60 index=0");
    let vs_constants = find("  render 46 length=16 index=30");
    let pos_fixup = find("  render 46 length=20 index=28");
    let vs_draw = find("  render 46 length=32 index=27");
    let ps_constants = find("  render 11 length=16 index=15");
    let draw = find(
        "  render 19 primitive_type=3 vertex_start=0 vertex_count=3 instance_count=1 \
         base_instance=0",
    );
    for bind in [vertices, vs_constants, pos_fixup, vs_draw, ps_constants] {
        assert!(pso < bind && bind < draw, "{calls:#?}");
    }
}

/// One pipeline per key: the same key twice compiles once, another write mask compiles another.
#[test]
fn ff_pipeline_compiles_once_per_key() {
    const DEVICE: u64 = 0xb2;
    let mut core = core_on_fakes(DEVICE);
    assert_eq!(core.draw_up(&key(0xf), &triangle()), Ok(()));
    assert_eq!(core.draw_up(&key(0xf), &triangle()), Ok(()));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
    assert_eq!(core.draw_up(&key(0x7), &triangle()), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
}

/// A pipeline that does not build drops its draws, and is not built again.
#[test]
fn ff_draw_with_a_failed_pipeline_records_nothing() {
    const DEVICE: u64 = dxmt_fakes::REFUSES_PIPELINES;
    let mut core = core_on_fakes(DEVICE);
    let refused = Err(super::DrawRefused::Pipeline);
    assert_eq!(core.draw_up(&key(0xf), &triangle()), refused);
    assert_eq!(core.draw_up(&key(0xf), &triangle()), refused);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(pipelines_made(&calls), 1, "{calls:#?}");
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("  render 15 ") || c.starts_with("  render 19 ")),
        "{calls:#?}"
    );
}
