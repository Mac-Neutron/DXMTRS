//! A core device on `dxmt-fakes` for the recorder's tests, and what its queue's commits captured.

use core::ffi::c_char;
use std::sync::{Mutex, Once, PoisonError};

use dxmt_fakes as _;
use mtld3d_core::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements},
    depth_stencil_state::DepthStencilSnapshot,
    dxso::MAX_LINKED_INPUTS,
    ff_state::FfState,
    pipeline_key::{
        CoreExtra, DeclCode, MAX_STREAMS, PipelineKey, PsStageKey, StreamKey, VsStageKey,
    },
    streams::stream_step,
};
use mtld3d_types::{
    D3DDECL_END, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_D3DCOLOR, D3DDECLTYPE_FLOAT4,
    D3DDECLUSAGE_COLOR, D3DDECLUSAGE_POSITIONT, D3DFVF_DIFFUSE, D3DFVF_XYZRHW, D3DVERTEXELEMENT9,
    render_state_defaults,
};
use winemetal_sys::{Obj, wire::WMTCompileOptions};

use super::{
    Bind, CoreDevice, CoreTextureDesc, CoreTextureFlags, DrawData, Indices, Stage, StreamData,
    TextureKind, queue_profile,
};

/// The library a sampled copy compiles in a test: a handle no fake makes and no test's device.
///
/// Below the fakes' first handle, so the functions made of it are its own and their calls, which
/// print nothing, are no test's.
const SOURCE_LIBRARY: u64 = 0xb117;

/// A core device on the made-up handle `handle` with a 64x32 back buffer and a window.
pub fn core_on_fakes(handle: u64) -> CoreDevice {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(handle) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile());
    core.attach(1, 64, 32, 1);
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
        flags: CoreTextureFlags::RENDER_TARGET,
        srgb_format: 0,
        block: (1, 1, texel_bytes),
        samples: 1,
    }
}

/// A square `size` one-level 2D render target of `pixel_format`, 4-byte texels of `samples`.
pub const fn ms_desc(pixel_format: u32, size: u32, samples: u8) -> CoreTextureDesc {
    CoreTextureDesc {
        samples,
        ..target_desc(pixel_format, TextureKind::Tex2d, size, 1, 4)
    }
}

/// [`ms_desc`]'s target, with its sRGB twin `srgb_format` and views in another format.
pub const fn twinned_ms_desc(
    pixel_format: u32,
    srgb_format: u32,
    size: u32,
    samples: u8,
) -> CoreTextureDesc {
    let mut desc = ms_desc(pixel_format, size, samples);
    desc.flags = desc.flags.union(CoreTextureFlags::VIEW_TWINS);
    desc.srgb_format = srgb_format;
    desc
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

/// The key entries of a UP draw at `stride` over a declaration of stream 0 alone.
pub fn inline(stride: u32) -> [StreamKey; MAX_STREAMS as usize] {
    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = StreamKey::Inline { stride };
    streams
}

/// A draw's streams: `bytes` on stream 0, nothing on the others.
pub const fn stream0(bytes: &[u8]) -> [StreamData<'_>; MAX_STREAMS as usize] {
    let mut streams = [StreamData::None; MAX_STREAMS as usize];
    streams[0] = StreamData::Bytes(bytes);
    streams
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
        streams: inline(stride),
        write_mask,
        color_format: winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
        depth_format: 0,
        blend: None,
        extras: [None, None, None],
        samples: 1,
        alpha_to_coverage: false,
    }
}

/// The fixed-function key of a pre-transformed position on stream 0 and a colour on stream 1.
///
/// The colour at `color_offset` of stream 1's element; the streams fed as `streams` says.
pub fn two_stream_key(
    color_offset: u16,
    streams: [StreamKey; MAX_STREAMS as usize],
) -> PipelineKey {
    let element = |stream, offset, type_, usage| D3DVERTEXELEMENT9 {
        stream,
        offset,
        type_,
        method: D3DDECLMETHOD_DEFAULT,
        usage,
        usage_index: 0,
    };
    let elements = [
        element(0, 0, D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITIONT),
        element(1, color_offset, D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR),
    ];
    let rs = render_state_defaults();
    let layout = ff_vs_layout_from_elements(&elements);
    PipelineKey {
        vs: VsStageKey::Ff(FfState::new().build_vs_key(&rs, layout, 0, [0; MAX_LINKED_INPUTS])),
        layout: decl(&elements),
        streams,
        ..key(0xf)
    }
}

/// The key entry of a bound stream: its stride and the step and rate of its frequency word.
pub const fn bound(stride: u32, freq: u32) -> StreamKey {
    let (step, rate) = stream_step(freq);
    StreamKey::Bound {
        stride,
        step,
        rate,
        shift: 0,
    }
}

/// The fixture's key, colour 0 written whole, naming `extras` at index 1-3.
pub fn key_extras(extras: [Option<CoreExtra>; 3]) -> PipelineKey {
    PipelineKey { extras, ..key(0xf) }
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
        streams: stream0(&VERTICES),
        vertex_count,
        vertex_start: 0,
        base_vertex: 0,
        instancing: None,
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
        blend_color: None,
        key_gen: 0,
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
        streams: stream0(&VERTICES[..60]),
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
        flags: CoreTextureFlags::RENDER_TARGET,
        srgb_format: 0,
        block: (1, 1, 4),
        samples: 1,
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

/// The vertex descriptor's lines (`  attr`, `  layout`) of each pipeline `calls` shows made.
pub fn descriptors(calls: &[String]) -> Vec<Vec<&str>> {
    let mut found = Vec::new();
    for call in calls {
        if call == "MTLDevice_newRenderPipelineStateWithVertexDescriptor" {
            found.push(Vec::new());
        } else if (call.starts_with("  attr ") || call.starts_with("  layout "))
            && let Some(last) = found.last_mut()
        {
            last.push(call.as_str());
        }
    }
    found
}

/// The first line of `pass` starting with `prefix`, without it.
pub fn line<'a>(pass: &'a [String], prefix: &str) -> Option<&'a str> {
    pass.iter().find_map(|c| c.strip_prefix(prefix))
}

/// `MTLDevice_newLibraryWithSourceOptions`, which the fakes leave out: [`SOURCE_LIBRARY`].
///
/// The test binary links with dynamic lookup, so the fixture defines the one winemetal call the
/// recorder makes that `dxmt-fakes` does not, as `dxmt-core`'s own tests define theirs. Every
/// source compiles; the options, the source and the error slot are not read.
#[unsafe(export_name = "MTLDevice_newLibraryWithSourceOptions")]
const extern "C-unwind" fn library_with_source(
    _device: u64,
    _source: *const c_char,
    _length: u64,
    _options: *const WMTCompileOptions,
    _error: *mut u64,
) -> u64 {
    SOURCE_LIBRARY
}

/// Process-wide test lock serializing tests that reset `dxmt_fakes`'s global shader cache or logs.
pub static FAKE_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

static TEST_LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct TestLogger;

impl log::Log for TestLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let mut logs = TEST_LOGS.lock().unwrap_or_else(PoisonError::into_inner);
            logs.push(record.args().to_string());
            drop(logs);
        }
    }

    fn flush(&self) {}
}

static TEST_LOGGER: TestLogger = TestLogger;
static LOGGER_INIT: Once = Once::new();

/// Installs the shared `d3d9-rt` unit-test logger once per test binary.
pub fn ensure_test_logger() {
    LOGGER_INIT.call_once(|| {
        let _ = log::set_logger(&TEST_LOGGER);
        log::set_max_level(log::LevelFilter::Info);
    });
}

/// Runs `f` with `ensure_test_logger` active and returns every log line appended during `f`.
pub fn capture_logs<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    ensure_test_logger();
    let start = {
        let logs = TEST_LOGS.lock().unwrap_or_else(PoisonError::into_inner);
        let len = logs.len();
        drop(logs);
        len
    };
    let res = f();
    let lines = {
        let logs = TEST_LOGS.lock().unwrap_or_else(PoisonError::into_inner);
        let slice = logs.get(start..).unwrap_or_default().to_vec();
        drop(logs);
        slice
    };
    (res, lines)
}
