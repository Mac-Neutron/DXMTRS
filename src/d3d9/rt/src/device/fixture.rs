//! A core device on `dxmt-fakes` for the recorder's tests, and what its queue's commits captured.

use dxmt_fakes as _;
use mtld3d_core::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements},
    depth_stencil_state::DepthStencilSnapshot,
    dxso::MAX_LINKED_INPUTS,
    ff_state::FfState,
    pipeline_key::{DeclCode, PipelineKey, PsStageKey, VsStageKey},
};
use mtld3d_types::{
    D3DDECL_END, D3DFVF_DIFFUSE, D3DFVF_XYZRHW, D3DVERTEXELEMENT9, render_state_defaults,
};
use winemetal_sys::Obj;

use super::{
    Bind, CoreDevice, CoreTextureDesc, DrawData, Indices, Stage, TextureKind, queue_profile,
};

/// A core device on the made-up handle `handle` with a 64x32 back buffer and a window.
pub fn core_on_fakes(handle: u64) -> CoreDevice {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(handle) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile());
    core.attach(1, 64, 32);
    core
}

/// The index of the first captured call equal to `line`; panics with every call when none is.
pub fn at(calls: &[String], line: &str) -> usize {
    calls
        .iter()
        .position(|c| c == line)
        .unwrap_or_else(|| panic!("no {line:?} in {calls:#?}"))
}

/// Runs `f` on a thread of its own and fails the test when it has not returned within 10 s.
pub fn returns_within_10s(f: impl FnOnce() + Send + 'static) {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = done.send(());
    });
    finished
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("returns within 10 s");
}

/// A square `size` render-target texture's description: `levels` levels, `texel_bytes` a texel.
pub const fn target_desc(
    pixel_format: u32,
    kind: TextureKind,
    size: u32,
    levels: u32,
    texel_bytes: u32,
) -> CoreTextureDesc {
    CoreTextureDesc {
        pixel_format,
        kind,
        width: size,
        height: size,
        depth: 1,
        levels,
        view_twins: false,
        block: (1, 1, texel_bytes),
        render_target: true,
    }
}

/// The handles (`#<n>`) of the textures `calls` shows made, in order.
pub fn textures_made(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("MTLDevice_newTexture "))
        .filter_map(|c| c.split(' ').next())
        .map(str::to_owned)
        .collect()
}

/// Each render encoder's colour target in `calls`, in order: its handle (`#<n>`) and its size.
pub fn targets(calls: &[String]) -> Vec<(String, String)> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("  target0 "))
        .filter_map(|c| c.split_once(' '))
        .map(|(handle, size)| (handle.to_owned(), size.to_owned()))
        .collect()
}

/// The declaration of `elements`, `D3DDECL_END` appended.
pub fn decl(elements: &[D3DVERTEXELEMENT9]) -> DeclCode {
    DeclCode::new(elements.iter().copied().chain([D3DDECL_END]).collect())
}

/// The key of a default-state `XYZRHW | DIFFUSE` draw, 20 bytes a vertex, with no depth-stencil.
pub fn key(write_mask: u8) -> PipelineKey {
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
        color_format: winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
        depth_format: 0,
    }
}

/// A full-target viewport of the fixture's 64x32 back buffer.
pub const VIEWPORT: [f64; 6] = [0.0, 0.0, 64.0, 32.0, 0.0, 1.0];

pub static VERTICES: [u8; 64] = [0; 64];

/// A draw of `primitive` over `vertex_count` of [`VERTICES`] with `binds`, unculled.
///
/// Its depth-stencil state is the inert one, its stencil reference and slope scale 0.
pub const fn draw<'a>(
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
        // Larger than any pass: the clamp makes it the pass's extent.
        scissor: [0, 0, u32::MAX, u32::MAX],
        fill_mode: 0,
        cull_mode: 0,
        binds,
        textures: &[],
        depth_stencil: DepthStencilSnapshot::inert(),
        stencil_ref: 0,
        slope_scale: 0.0,
    }
}

/// A fixed-function triangle's binds, as C4a's arm makes them.
pub fn ff_binds() -> [Bind<'static>; 6] {
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
pub fn triangle<'a>(binds: &'a [Bind<'a>]) -> DrawData<'a> {
    DrawData {
        vertices: &VERTICES[..60],
        ..draw(3, 3, None, binds)
    }
}

/// A depth texture's description: `pixel_format` is a depth-stencil `WMTPixelFormat`.
pub const fn depth_desc(
    pixel_format: u32,
    width: u32,
    height: u32,
    levels: u32,
) -> CoreTextureDesc {
    CoreTextureDesc {
        pixel_format,
        kind: TextureKind::Tex2d,
        width,
        height,
        depth: 1,
        levels,
        view_twins: false,
        block: (1, 1, 4),
        render_target: true,
    }
}

/// Each render pass in `calls`, in order: its lines from its encoder to before its end.
pub fn passes(calls: &[String]) -> Vec<&[String]> {
    let mut found = Vec::new();
    let mut rest = calls;
    while let Some(start) = rest
        .iter()
        .position(|c| c == "MTLCommandBuffer_renderCommandEncoder")
    {
        let len = rest[start..]
            .iter()
            .position(|c| c == "MTLCommandEncoder_endEncoding")
            .unwrap_or(rest.len() - start);
        found.push(&rest[start..start + len]);
        rest = &rest[start + len..];
    }
    found
}

/// The first line of `pass` starting with `prefix`, without it.
pub fn line<'a>(pass: &'a [String], prefix: &str) -> Option<&'a str> {
    pass.iter().find_map(|c| c.strip_prefix(prefix))
}
