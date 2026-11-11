use airconv_ir::dxso::spec::VariantKey;

use super::FfDraw;
use crate::device::fixture::{at, core_on_fakes};

/// `vs_3_0` and `ps_3_0` stand-ins: the fakes translate any tokens.
const VS: [u32; 2] = [0xfffe_0300, 0x0000_ffff];
const PS: [u32; 2] = [0xffff_0300, 0x0000_ffff];

/// An `XYZRHW | DIFFUSE` triangle: three 20-byte vertices.
fn triangle(write_mask: u8) -> FfDraw<'static> {
    static VERTICES: [u8; 60] = [0; 60];
    static VS_CONSTANTS: [u8; 16] = [0; 16];
    static VS_DRAW: [u8; 32] = [0; 32];
    static PS_CONSTANTS: [u8; 16] = [0; 16];
    static ALPHA_REF: [u8; 4] = [0; 4];
    static FOG: [u8; 32] = [0; 32];
    FfDraw {
        vs_tokens: &VS,
        vs_inputs: 0b1_0001,
        clip_planes: 0,
        ps_tokens: &PS,
        variant: VariantKey::default(),
        attributes: &[(0, 31, 0), (4, 42, 16)],
        stride: 20,
        write_mask,
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

/// A fixed-function `DrawPrimitiveUP` reaches Metal in the cleared pass.
///
/// Its pipeline, the vertex bytes at buffer 0, the stages' constants at D3D9's slots, then one
/// draw, in the render pass the Clear before it became the load action of.
#[test]
fn ff_draw_up_records_pipeline_binds_and_draw() {
    const DEVICE: u64 = 0xb1;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0]);
    assert!(core.draw_up(&triangle(0xf)));
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

/// One pipeline per key: the same draw twice builds it once, another write mask builds another.
#[test]
fn ff_pipeline_is_built_once_per_key() {
    const DEVICE: u64 = 0xb2;
    let mut core = core_on_fakes(DEVICE);
    assert!(core.draw_up(&triangle(0xf)));
    assert!(core.draw_up(&triangle(0xf)));
    assert!(core.draw_up(&triangle(0x7)));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let built = calls
        .iter()
        .filter(|c| *c == "MTLDevice_newRenderPipelineStateWithVertexDescriptor")
        .count();
    assert_eq!(built, 2, "{calls:#?}");
}
