use core::{
    ffi::{CStr, c_char},
    fmt::Write,
    mem::{offset_of, size_of},
    sync::atomic::{
        AtomicU64, AtomicUsize,
        Ordering::{Release, SeqCst},
    },
};
use std::{
    collections::VecDeque,
    sync::MutexGuard,
    thread,
    time::{Duration, Instant},
};

use winemetal_sys::wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_BGRX8_UNORM,
    WMT_PIXEL_FORMAT_BGRX8_UNORM_S_RGB, WMT_PIXEL_FORMAT_RGB10A2_UNORM,
    WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB,
    WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMT_RENDER_COMMAND_UPDATE_FENCE,
    WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_VERTEX,
    WmtcmdRenderFenceOp,
};

use super::*;

/// The C++ presenter's recorded run: the scenarios.
///
/// `golden/presenter.cpp` over `dxmt_presenter.cpp`; `GOLDEN_CALLS` is what its stubs printed.
const GOLDEN_MATRIX: &str = include_str!("golden/matrix.txt");
const GOLDEN_CALLS: &str = include_str!("golden/calls.txt");

/// The handles the C++ golden passes in, named as it names them.
const DEVICE: u64 = 0xD1;
const LAYER: u64 = 0x1A;
const CMDBUF: u64 = 0xC0;
const BACKBUFFER: u64 = 0xBB;
const LIBRARY: u64 = 0x11B;

/// What the fakes answer and print.
///
/// A static, because the fakes are winemetal's C functions, which the presenter calls with no
/// object of the test's to reach. A test that uses them holds [`SERIAL`] and starts with
/// [`reset`].
static FAKE: Mutex<Fake> = Mutex::new(Fake::new());

/// Held by each test that uses the fakes: they are one process-wide set.
static SERIAL: Mutex<()> = Mutex::new(());

/// The display queries answered, each after its line.
///
/// It orders the golden's `syncwait` thread against the scenario's.
static QUERIES: AtomicUsize = AtomicUsize::new(0);

struct Fake {
    out: String,
    names: Vec<(u64, String)>,
    counters: Vec<(&'static str, u32)>,
    next: u64,
    textures: Vec<(u64, u64)>,
    display: (u64, u64, WMTHDRMetadata, WMTEDRValue),
    layer: WMTLayerProps,
    source: (u64, u64),
}

impl Fake {
    const fn new() -> Self {
        Self {
            out: String::new(),
            names: Vec::new(),
            counters: Vec::new(),
            next: 0x1000,
            textures: Vec::new(),
            display: (
                0,
                WMT_COLOR_SPACE_SRGB,
                NO_HDR_METADATA,
                WMTEDRValue {
                    maximum_edr_color_component_value: 1.0,
                    maximum_potential_edr_color_component_value: 1.0,
                },
            ),
            layer: layer_props(2560.0, 1440.0, 2.0),
            source: (0, 0),
        }
    }

    /// A new handle named `<kind>#<n>`.
    fn handle(&mut self, kind: &'static str) -> u64 {
        let h = self.next;
        self.next += 1;
        let n = if let Some(c) = self.counters.iter_mut().find(|c| c.0 == kind) {
            c.1 += 1;
            c.1
        } else {
            self.counters.push((kind, 1));
            1
        };
        self.names.push((h, format!("{kind}#{n}")));
        h
    }

    fn name(&self, h: u64) -> String {
        let fixed = match h {
            0 => "null",
            DEVICE => "device",
            LAYER => "layer",
            CMDBUF => "cmdbuf",
            BACKBUFFER => "backbuffer",
            LIBRARY => "library",
            _ => "",
        };
        if !fixed.is_empty() {
            return fixed.to_owned();
        }
        self.names
            .iter()
            .find(|n| n.0 == h)
            .map_or_else(|| format!("0x{h:x}"), |n| n.1.clone())
    }

    fn say(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }
}

/// `MetalLayer_getProps`'s answer: the golden's layer of `width` × `height` at `scale`.
const fn layer_props(width: f64, height: f64, scale: f64) -> WMTLayerProps {
    WMTLayerProps {
        device: 0,
        contents_scale: scale,
        drawable_width: width,
        drawable_height: height,
        opaque: 0,
        display_sync_enabled: 1,
        framebuffer_only: 1,
        pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
    }
}

fn fake() -> MutexGuard<'static, Fake> {
    FAKE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets every handle and line, as the golden does at each scenario.
fn reset() {
    *fake() = Fake::new();
}

fn say(line: &str) {
    fake().say(line);
}

fn take() -> String {
    core::mem::take(&mut fake().out)
}

/// `printf(format, value)` for one `double` conversion.
fn c_format(format: &CStr, value: f64) -> String {
    let mut buf = [0_u8; 64];
    // SAFETY: a buffer of its length; a format with one `double` conversion.
    let n = unsafe { libc::snprintf(buf.as_mut_ptr().cast(), buf.len(), format.as_ptr(), value) };
    String::from_utf8_lossy(&buf[..usize::try_from(n).expect("a length")]).into_owned()
}

/// `%g`.
fn g(value: f64) -> String {
    c_format(c"%g", value)
}

fn colorspace_name(cs: u64) -> &'static str {
    match cs {
        WMT_COLOR_SPACE_SRGB => "sRGB",
        WMT_COLOR_SPACE_SRGB_LINEAR => "sRGBLinear",
        WMT_COLOR_SPACE_BT2020 => "BT2020",
        WMT_COLOR_SPACE_HDR_PQ => "HDR_PQ",
        WMT_COLOR_SPACE_HDR_SC_RGB => "HDR_scRGB",
        WMT_COLOR_SPACE_INVALID => "Invalid",
        _ => "?",
    }
}

fn hdr_text(m: &WMTHDRMetadata) -> String {
    format!(
        "{{r={},{} g={},{} b={},{} w={},{} maxmaster={} minmaster={} maxcll={} maxfall={}}}",
        m.red_primary[0],
        m.red_primary[1],
        m.green_primary[0],
        m.green_primary[1],
        m.blue_primary[0],
        m.blue_primary[1],
        m.white_point[0],
        m.white_point[1],
        m.max_mastering_luminance,
        m.min_mastering_luminance,
        m.max_content_light_level,
        m.max_frame_average_light_level
    )
}

fn props_text(f: &Fake, p: &WMTLayerProps) -> String {
    format!(
        "{{device={} contents_scale={} drawable={}x{} opaque={} display_sync={} \
         framebuffer_only={} pixel_format=0x{:x}}}",
        f.name(p.device),
        g(p.contents_scale),
        g(p.drawable_width),
        g(p.drawable_height),
        p.opaque,
        p.display_sync_enabled,
        p.framebuffer_only,
        p.pixel_format
    )
}

/// FNV-1a over `bytes`.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The bytes a `WMTMemoryPointer` points to.
///
/// # Safety
///
/// `m` points to `len` readable bytes.
unsafe fn bytes<'a>(m: MemPtr, len: usize) -> &'a [u8] {
    let p = core::ptr::with_exposed_provenance::<u8>(usize::try_from(m.ptr).expect("an address"));
    // SAFETY: the caller's promise.
    unsafe { core::slice::from_raw_parts(p, len) }
}

/// The value of a C string argument.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
unsafe fn text(s: *const c_char) -> String {
    // SAFETY: the caller's promise.
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_pool() -> u64 {
    let mut f = fake();
    let h = f.handle("pool");
    let line = format!("  NSAutoreleasePool_alloc_init -> {}", f.name(h));
    f.say(&line);
    h
}

#[unsafe(export_name = "MetalLayer_getProps")]
extern "C-unwind" fn fake_get_props(layer: u64, props: *mut WMTLayerProps) {
    let mut f = fake();
    // SAFETY: the presenter's props, writable.
    unsafe { props.write(f.layer) };
    let line = format!(
        "  MetalLayer_getProps {} -> {}",
        f.name(layer),
        props_text(&f, &f.layer)
    );
    f.say(&line);
}

#[unsafe(export_name = "MetalLayer_setProps")]
extern "C-unwind" fn fake_set_props(layer: u64, props: *const WMTLayerProps) {
    let mut f = fake();
    // SAFETY: the presenter's props, readable.
    let props = unsafe { &*props };
    let line = format!(
        "  MetalLayer_setProps {} {}",
        f.name(layer),
        props_text(&f, props)
    );
    f.say(&line);
}

#[unsafe(export_name = "MetalLayer_setColorSpace")]
extern "C-unwind" fn fake_set_color_space(layer: u64, cs: u64) -> bool {
    let mut f = fake();
    let line = format!(
        "  MetalLayer_setColorSpace {} {}",
        f.name(layer),
        colorspace_name(cs)
    );
    f.say(&line);
    true
}

#[unsafe(export_name = "MetalLayer_nextDrawable")]
extern "C-unwind" fn fake_next_drawable(layer: u64) -> u64 {
    let mut f = fake();
    let h = f.handle("drawable");
    let line = format!(
        "  MetalLayer_nextDrawable {} -> {}",
        f.name(layer),
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MetalDrawable_texture")]
extern "C-unwind" fn fake_drawable_texture(drawable: u64) -> u64 {
    let mut f = fake();
    let known = f.textures.iter().find(|t| t.0 == drawable).map(|t| t.1);
    let t = known.unwrap_or_else(|| {
        let t = f.handle("drawable-texture");
        f.textures.push((drawable, t));
        t
    });
    let line = format!(
        "  MetalDrawable_texture {} -> {}",
        f.name(drawable),
        f.name(t)
    );
    f.say(&line);
    t
}

#[unsafe(export_name = "MTLDevice_newTexture")]
extern "C-unwind" fn fake_new_texture(device: u64, info: *mut WMTTextureInfo) -> u64 {
    let mut f = fake();
    // SAFETY: the presenter's info, readable.
    let i = unsafe { &*info };
    let h = f.handle("texture");
    let line = format!(
        "  MTLDevice_newTexture {} {{format=0x{:x} w={} h={} d={} array={} type={} mips={} \
         samples={} usage={} options=0x{:x} flags={} mach_port={}}} -> {}",
        f.name(device),
        i.pixel_format,
        i.width,
        i.height,
        i.depth,
        i.array_length,
        i.r#type(),
        i.mipmap_level_count(),
        i.sample_count(),
        i.usage(),
        i.options & 0xffff_ffff,
        i.flags,
        i.mach_port,
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MTLTexture_replaceRegion")]
extern "C-unwind" fn fake_replace_region(
    texture: u64,
    o: WMTOrigin,
    s: WMTSize,
    level: u64,
    slice: u64,
    data: MemPtr,
    bytes_per_row: u64,
    bytes_per_image: u64,
) {
    // SAFETY: the presenter's row, `bytes_per_row` bytes.
    let row = unsafe { bytes(data, usize::try_from(bytes_per_row).expect("a length")) };
    let mut f = fake();
    let line = format!(
        "  MTLTexture_replaceRegion {} origin={},{},{} size={}x{}x{} level={level} slice={slice} \
         bytes_per_row={bytes_per_row} bytes_per_image={bytes_per_image} data_fnv={:016x}",
        f.name(texture),
        o.x,
        o.y,
        o.z,
        s.width,
        s.height,
        s.depth,
        fnv(row)
    );
    f.say(&line);
}

#[unsafe(export_name = "MTLTexture_width")]
extern "C-unwind" fn fake_width(texture: u64) -> u64 {
    if texture == BACKBUFFER {
        fake().source.0
    } else {
        0
    }
}

#[unsafe(export_name = "MTLTexture_height")]
extern "C-unwind" fn fake_height(texture: u64) -> u64 {
    if texture == BACKBUFFER {
        fake().source.1
    } else {
        0
    }
}

#[unsafe(export_name = "MTLLibrary_newFunction")]
extern "C-unwind" fn fake_new_function(library: u64, name: *const c_char) -> u64 {
    // SAFETY: the presenter's name, NUL-terminated.
    let name = unsafe { text(name) };
    let mut f = fake();
    let h = f.handle("function");
    let line = format!(
        "  MTLLibrary_newFunction {} \"{name}\" -> {}",
        f.name(library),
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MTLLibrary_newFunctionWithConstants")]
extern "C-unwind" fn fake_new_function_with_constants(
    library: u64,
    name: *const c_char,
    constants: *const WMTFunctionConstant,
    n: u32,
    _error: *mut u64,
) -> u64 {
    // SAFETY: the presenter's name, NUL-terminated.
    let name = unsafe { text(name) };
    // SAFETY: the presenter's `n` constants.
    let constants =
        unsafe { core::slice::from_raw_parts(constants, usize::try_from(n).expect("a count")) };
    let mut list = String::new();
    for c in constants {
        // SAFETY: each constant's data is a `uint32_t` of the presenter's.
        let value = u32::from_ne_bytes(unsafe { bytes(c.data, 4) }.try_into().expect("4 bytes"));
        write!(
            list,
            " [index=0x{:x} type={} value={value}]",
            c.index, c.r#type
        )
        .expect("a String");
    }
    let mut f = fake();
    let h = f.handle("function");
    let line = format!(
        "  MTLLibrary_newFunctionWithConstants {} \"{name}\" n={n}{list} -> {}",
        f.name(library),
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MTLDevice_newRenderPipelineState")]
extern "C-unwind" fn fake_new_pipeline(
    device: u64,
    info: *const WMTRenderPipelineInfo,
    _error: *mut u64,
) -> u64 {
    // SAFETY: the presenter's info, readable.
    let i = unsafe { &*info };
    let mut f = fake();
    let h = f.handle("pso");
    let mut colors = String::new();
    for (k, c) in i.colors.iter().enumerate() {
        if k == 0 || c.pixel_format != 0 {
            write!(
                colors,
                " c{k}={{fmt=0x{:x} rgb_op={} alpha_op={} src_rgb={} dst_rgb={} src_a={} dst_a={} \
                 mask={} blend={}}}",
                c.pixel_format,
                c.rgb_blend_operation,
                c.alpha_blend_operation,
                c.src_rgb_blend_factor,
                c.dst_rgb_blend_factor,
                c.src_alpha_blend_factor,
                c.dst_alpha_blend_factor,
                c.write_mask,
                c.blending_enabled
            )
            .expect("a String");
        }
    }
    let line = format!(
        "  MTLDevice_newRenderPipelineState {}{colors} alpha_to_coverage={} logic={}/{} raster={} \
         samples={} depth_fmt=0x{:x} stencil_fmt=0x{:x} vs={} fs={} immutable_vb={} \
         immutable_fb={} topology={} tess={}/{}/{}/{} archive={} lookup={}/{} \
         fail_on_miss={} icb={} -> {}",
        f.name(device),
        i.alpha_to_coverage_enabled,
        i.logic_operation_enabled,
        i.logic_operation,
        i.rasterization_enabled,
        i.raster_sample_count,
        i.depth_pixel_format,
        i.stencil_pixel_format,
        f.name(i.vertex_function),
        f.name(i.fragment_function),
        i.immutable_vertex_buffers,
        i.immutable_fragment_buffers,
        i.input_primitive_topology,
        i.tessellation_partition_mode,
        i.max_tessellation_factor,
        i.tessellation_output_winding_order,
        i.tessellation_factor_step,
        f.name(i.binary_archive_for_serialization),
        i.num_binary_archives_for_lookup,
        u8::from(i.binary_archives_for_lookup.ptr != 0),
        i.fail_on_binary_archive_miss,
        i.support_indirect_command_buffers,
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MTLCommandBuffer_renderCommandEncoder")]
extern "C-unwind" fn fake_render_encoder(cmdbuf: u64, info: *mut WMTRenderPassInfo) -> u64 {
    // SAFETY: the presenter's info, readable, every byte written (its padding zeroed).
    let whole =
        unsafe { core::slice::from_raw_parts(info.cast::<u8>(), size_of::<WMTRenderPassInfo>()) };
    // SAFETY: as above.
    let i = unsafe { &*info };
    let c = &i.colors[0];
    let mut f = fake();
    let h = f.handle("encoder");
    let line = format!(
        "  MTLCommandBuffer_renderCommandEncoder {} color0={{tex={} load={} store={} level={} \
         slice={} depth_plane={} clear={},{},{},{} resolve={},{},{},{}}} \
         depth={{tex={} load={} store={} clear={}}} stencil={{tex={} load={} store={} clear={}}} \
         default_samples={} rt_array={} tile={}x{} rt={}x{} vis={},{} \
         whole_struct_fnv={:016x} -> {}",
        f.name(cmdbuf),
        f.name(c.texture),
        c.load_action,
        c.store_action,
        c.level,
        c.slice,
        c.depth_plane,
        g(c.clear_color.r),
        g(c.clear_color.g),
        g(c.clear_color.b),
        g(c.clear_color.a),
        f.name(c.resolve_texture),
        c.resolve_level,
        c.resolve_slice,
        c.resolve_depth_plane,
        f.name(i.depth.texture),
        i.depth.load_action,
        i.depth.store_action,
        g(f64::from(i.depth.clear_depth)),
        f.name(i.stencil.texture),
        i.stencil.load_action,
        i.stencil.store_action,
        i.stencil.clear_stencil,
        i.default_raster_sample_count,
        i.render_target_array_length,
        i.tile_width,
        i.tile_height,
        i.render_target_width,
        i.render_target_height,
        i.visibility_accumulate,
        f.name(i.visibility_buffer),
        fnv(whole),
        f.name(h)
    );
    f.say(&line);
    h
}

#[unsafe(export_name = "MTLCommandEncoder_endEncoding")]
extern "C-unwind" fn fake_end_encoding(encoder: u64) {
    let mut f = fake();
    let line = format!("  MTLCommandEncoder_endEncoding {}", f.name(encoder));
    f.say(&line);
}

/// One record of a `wmtcmd` list, as the golden prints it.
///
/// # Safety
///
/// `c` is a render record of the presenter's (or a fence record of the test's).
unsafe fn record(f: &Fake, encoder: &str, c: *const WmtcmdBase) -> String {
    // SAFETY: the caller's promise.
    let kind = unsafe { (*c).r#type };
    match kind {
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE => {
            // SAFETY: a record of that type.
            let r = unsafe { &*c.cast::<WmtcmdRenderSettexture>() };
            format!(
                "  record {encoder} SetFragmentTexture texture={} index={}",
                f.name(r.texture),
                r.index
            )
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES => {
            // SAFETY: as above.
            let r = unsafe { &*c.cast::<WmtcmdRenderSetbytes>() };
            // SAFETY: the presenter's `length` bytes.
            let data = unsafe { bytes(r.bytes, usize::try_from(r.length).expect("a length")) };
            let mut hex = String::new();
            for b in data {
                write!(hex, "{b:02x}").expect("a String");
            }
            format!(
                "  record {encoder} SetFragmentBytes length={} index={} bytes={hex}",
                r.length, r.index
            )
        }
        WMT_RENDER_COMMAND_SET_PSO => {
            // SAFETY: as above.
            let r = unsafe { &*c.cast::<WmtcmdRenderSetpso>() };
            format!("  record {encoder} SetPSO pso={}", f.name(r.pso))
        }
        WMT_RENDER_COMMAND_SET_VIEWPORTS => {
            // SAFETY: as above.
            let r = unsafe { &*c.cast::<WmtcmdRenderSetviewports>() };
            let size = size_of::<WMTViewport>() * usize::from(r.viewport_count);
            // SAFETY: the presenter's viewports.
            let raw = unsafe { bytes(r.viewports, size) };
            let mut line = format!("  record {encoder} SetViewports count={}", r.viewport_count);
            for v in raw.chunks(size_of::<WMTViewport>()) {
                let d: Vec<f64> = v
                    .chunks(8)
                    .map(|b| f64::from_ne_bytes(b.try_into().expect("8 bytes")))
                    .collect();
                write!(
                    line,
                    " {{origin={},{} size={}x{} z={}..{}}}",
                    g(d[0]),
                    g(d[1]),
                    g(d[2]),
                    g(d[3]),
                    g(d[4]),
                    g(d[5])
                )
                .expect("a String");
            }
            line
        }
        WMT_RENDER_COMMAND_DRAW => {
            // SAFETY: as above.
            let r = unsafe { &*c.cast::<WmtcmdRenderDraw>() };
            format!(
                "  record {encoder} Draw primitive={} vertex_start={} vertex_count={} \
                 instance_count={} base_instance={}",
                r.primitive_type, r.vertex_start, r.vertex_count, r.instance_count, r.base_instance
            )
        }
        WMT_RENDER_COMMAND_WAIT_FOR_FENCE | WMT_RENDER_COMMAND_UPDATE_FENCE => {
            // SAFETY: as above.
            let r = unsafe { &*c.cast::<WmtcmdRenderFenceOp>() };
            let op = if kind == WMT_RENDER_COMMAND_WAIT_FOR_FENCE {
                "WaitForFence"
            } else {
                "UpdateFence"
            };
            format!(
                "  record {encoder} {op} fence={} stages={}",
                f.name(r.fence),
                r.stages
            )
        }
        _ => format!("  record {encoder} type={kind} (not decoded)"),
    }
}

#[unsafe(export_name = "MTLRenderCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_encode_commands(encoder: u64, head: *const WmtcmdBase) {
    let mut f = fake();
    let name = f.name(encoder);
    let mut c = head;
    while !c.is_null() {
        // SAFETY: a record of the list the caller passed.
        let line = unsafe { record(&f, &name, c) };
        f.say(&line);
        // SAFETY: as above.
        let next = unsafe { (*c).next.ptr };
        c = core::ptr::with_exposed_provenance(usize::try_from(next).expect("an address"));
    }
    drop(f);
}

#[unsafe(export_name = "WMTQueryDisplaySettingForLayer")]
extern "C-unwind" fn fake_query_display(
    layer: u64,
    version: *mut u64,
    cs: *mut u64,
    metadata: *mut WMTHDRMetadata,
    edr: *mut WMTEDRValue,
) {
    let mut f = fake();
    let (answer, colorspace, hdr, edr_value) = f.display;
    // SAFETY: the presenter's out pointers, each writable.
    unsafe { version.write(answer) };
    // SAFETY: as above.
    unsafe { cs.write(colorspace) };
    // SAFETY: as above.
    unsafe { metadata.write(hdr) };
    // SAFETY: as above.
    unsafe { edr.write(edr_value) };
    let line = format!(
        "  WMTQueryDisplaySettingForLayer {} -> version={answer} cs={} hdr={} edr={}/{}",
        f.name(layer),
        colorspace_name(colorspace),
        hdr_text(&hdr),
        g(f64::from(edr_value.maximum_edr_color_component_value)),
        g(f64::from(
            edr_value.maximum_potential_edr_color_component_value
        ))
    );
    f.say(&line);
    drop(f);
    QUERIES.fetch_add(1, SeqCst);
}

/// The golden's metadata `some` and `big`; anything else is none.
fn hdr(word: &str) -> WMTHDRMetadata {
    let (mastering, min, content, average) = match word {
        "some" => (1000, 1, 800, 400),
        "big" => (4000, 5, 9000, 700),
        _ => return NO_HDR_METADATA,
    };
    WMTHDRMetadata {
        red_primary: [34000, 16000],
        green_primary: [13250, 34500],
        blue_primary: [7500, 3000],
        white_point: [15635, 16450],
        max_mastering_luminance: mastering,
        min_mastering_luminance: min,
        max_content_light_level: content,
        max_frame_average_light_level: average,
    }
}

fn colorspace(word: &str) -> u64 {
    match word {
        "sRGB" => WMT_COLOR_SPACE_SRGB,
        "sRGBLinear" => WMT_COLOR_SPACE_SRGB_LINEAR,
        "BT2020" => WMT_COLOR_SPACE_BT2020,
        "PQ" | "HDR_PQ" => WMT_COLOR_SPACE_HDR_PQ,
        "scRGB" | "HDR_scRGB" => WMT_COLOR_SPACE_HDR_SC_RGB,
        _ => WMT_COLOR_SPACE_INVALID,
    }
}

fn format(word: &str) -> u32 {
    match word {
        "BGRA8Unorm" => WMT_PIXEL_FORMAT_BGRA8_UNORM,
        "BGRA8Unorm_sRGB" => WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        "RGBA8Unorm" => WMT_PIXEL_FORMAT_RGBA8_UNORM,
        "RGBA8Unorm_sRGB" => WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB,
        "RGB10A2Unorm" => WMT_PIXEL_FORMAT_RGB10A2_UNORM,
        "RGBA16Float" => WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        "BGRX8Unorm" => WMT_PIXEL_FORMAT_BGRX8_UNORM,
        "BGRX8Unorm_sRGB" => WMT_PIXEL_FORMAT_BGRX8_UNORM_S_RGB,
        "RGBA32Float" => WMT_PIXEL_FORMAT_RGBA32_FLOAT,
        _ => WMT_PIXEL_FORMAT_INVALID,
    }
}

/// The golden's ramp `v1` (version 7) or `v2` (version 8).
///
/// Written through the atomics, as an output writes one.
fn ramp(v2: bool) -> Box<DXMTGammaRamp> {
    let ramp = Box::new(DXMTGammaRamp {
        red: [const { AtomicU32::new(0) }; GAMMA_CP_COUNT],
        green: [const { AtomicU32::new(0) }; GAMMA_CP_COUNT],
        blue: [const { AtomicU32::new(0) }; GAMMA_CP_COUNT],
        version: AtomicU64::new(0),
    });
    for i in 0..GAMMA_CP_COUNT {
        let x = u32_to_f32(u32::try_from(i).expect("an index")) / 1023.0;
        let (r, gr, b) = if v2 {
            (x.sqrt(), x * x, 1.0 - x)
        } else {
            (x, 1.0 - x, 0.5)
        };
        ramp.red[i].store(r.to_bits(), Relaxed);
        ramp.green[i].store(gr.to_bits(), Relaxed);
        ramp.blue[i].store(b.to_bits(), Relaxed);
    }
    // Out of range points the C++ clamps, and a NaN it passes through.
    ramp.red[3].store((-0.5_f32).to_bits(), Relaxed);
    ramp.green[5].store(1.5_f32.to_bits(), Relaxed);
    ramp.blue[7].store(f32::INFINITY.to_bits(), Relaxed);
    ramp.red[9].store(f32::NAN.to_bits(), Relaxed);
    ramp.version.store(if v2 { 8 } else { 7 }, Release);
    ramp
}

fn new_presenter(scale_factor: f32, sample_count: u8) -> Presenter {
    let desc = PresenterDesc {
        device: DEVICE,
        layer: LAYER,
        library: LIBRARY,
        scale_factor,
        sample_count,
    };
    // SAFETY: the fakes' objects, alive for the test.
    unsafe { Presenter::new(&desc) }
}

fn encode(p: &Presenter, metadata: &DXMTPresentMetadata) -> Option<Obj> {
    // SAFETY: the fakes' objects.
    unsafe { p.encode(CMDBUF, BACKBUFFER, metadata, |_| {}, |_| {}) }
}

/// A presenter whose first frame (id 1) was synchronised and is not done.
///
/// Its pipelines are built and the fakes' log is empty.
fn presenter() -> Presenter {
    reset();
    let p = new_presenter(1.0, 1);
    let _ = p.change_layer_properties(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_COLOR_SPACE_SRGB,
        1920.0,
        1080.0,
        1,
    );
    assert_eq!(p.synchronize().0, 1);
    take();
    p
}

fn pipelines_built(log: &str) -> usize {
    log.matches("MTLDevice_newRenderPipelineState").count()
}

/// Waits up to 2 s for `t` to end.
fn ends<T>(t: &thread::ScopedJoinHandle<'_, T>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !t.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    t.is_finished()
}

#[test]
fn sync_waits_for_every_requested_frame() {
    let _serial = serial();
    let p = presenter();
    assert!(p.change_layer_properties(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_COLOR_SPACE_SRGB,
        1920.0,
        1080.0,
        4
    ));
    thread::scope(|s| {
        let t = s.spawn(|| p.synchronize().0);
        thread::sleep(Duration::from_millis(200));
        assert!(!t.is_finished(), "frame 1 is not done: the rebuild waits");
        let releaser = s.spawn(|| p.frame_done(1));
        assert!(ends(&releaser));
        assert!(ends(&t), "frame 1 is done: the rebuild goes on");
        assert_eq!(t.join().expect("the sync"), 2);
    });
    assert_eq!(pipelines_built(&take()), 2);
}

#[test]
fn sync_wait_does_not_block_encode_state() {
    let _serial = serial();
    let p = presenter();
    let _ = p.change_layer_color_space(WMT_COLOR_SPACE_HDR_PQ);
    thread::scope(|s| {
        let a = s.spawn(|| p.synchronize().0);
        thread::sleep(Duration::from_millis(100));
        assert!(!a.is_finished(), "A waits for frame 1");
        let b = s.spawn(|| {
            let metadata = DXMTPresentMetadata {
                edr_scale: 1.0,
                max_content_luminance: 10000.0,
                max_display_luminance: 100.0,
            };
            drop(encode(&p, &metadata));
            p.frame_done(1);
        });
        assert!(ends(&b), "B encodes frame 1 while A waits");
        assert!(ends(&a));
        assert_eq!(a.join().expect("the sync"), 2);
    });
}

#[test]
fn frame_done_never_moves_back() {
    let _serial = serial();
    let p = presenter();
    let _ = p.synchronize();
    let _ = p.synchronize();
    p.frame_done(3);
    p.frame_done(1);
    p.frame_done(2);
    assert_eq!(*p.presented.lock().expect("the count"), 3);
    let _ = p.change_layer_color_space(WMT_COLOR_SPACE_BT2020);
    thread::scope(|s| {
        let t = s.spawn(|| p.synchronize().0);
        assert!(ends(&t), "every requested frame is done");
        assert_eq!(t.join().expect("the sync"), 4);
    });
}

#[test]
fn gamma_lut_is_the_cpps() {
    for v2 in [false, true] {
        let ramp = ramp(v2);
        let lut = gamma_lut(&ramp);
        assert_eq!(lut.len(), 4096);
        // std::clamp(v, 0.f, 1.f): v < 0 ? 0 : 1 < v ? 1 : v.
        let clamp = |v: f32| match (v < 0.0, 1.0 < v) {
            (true, _) => 0.0,
            (_, true) => 1.0,
            _ => v,
        };
        for i in 0..GAMMA_CP_COUNT {
            let point = |curve: &[AtomicU32; GAMMA_CP_COUNT]| {
                clamp(f32::from_bits(curve[i].load(Relaxed))).to_bits()
            };
            let want = [
                point(&ramp.red),
                point(&ramp.green),
                point(&ramp.blue),
                1.0_f32.to_bits(),
            ];
            let got: Vec<u32> = lut[i * 4..i * 4 + 4].iter().map(|v| v.to_bits()).collect();
            assert_eq!(got, want, "point {i} of ramp v{}", u8::from(v2) + 1);
        }
        assert!(lut[9 * 4].is_nan());
        assert_eq!(
            (lut[3 * 4], lut[5 * 4 + 1], lut[7 * 4 + 2]),
            (0.0, 1.0, 1.0)
        );
    }
}

#[test]
fn unchanged_properties_keep_the_pso() {
    let _serial = serial();
    let p = presenter();
    assert!(!p.change_layer_properties(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_COLOR_SPACE_SRGB,
        1280.0,
        720.0,
        1
    ));
    let _ = p.synchronize();
    let log = take();
    assert!(log.contains("MetalLayer_setProps layer"), "{log}");
    assert_eq!(pipelines_built(&log), 0, "{log}");
}

#[test]
fn changed_colorspace_marks_it() {
    let _serial = serial();
    let p = presenter();
    assert!(p.change_layer_color_space(WMT_COLOR_SPACE_HDR_PQ));
    assert!(!p.change_layer_color_space(WMT_COLOR_SPACE_HDR_PQ));
    p.frame_done(1);
    let _ = p.synchronize();
    let log = take();
    assert_eq!(pipelines_built(&log), 2);
    assert!(log.contains("[index=0x101 type=53 value=1]"), "{log}");
    assert!(
        log.contains("MetalLayer_setColorSpace layer HDR_PQ"),
        "{log}"
    );
}

#[test]
fn hdr_metadata_none_and_some() {
    let _serial = serial();
    let p = presenter();
    let some = hdr("some");
    let mut frame = 1;
    for (metadata, rebuilds) in [
        (None, false),
        (Some(&some), true),
        (Some(&some), true),
        (None, true),
        (None, false),
    ] {
        p.change_hdr_metadata(metadata);
        p.frame_done(frame);
        frame = p.synchronize().0;
        let built = pipelines_built(&take());
        assert_eq!(built == 2, rebuilds, "metadata {}", metadata.is_some());
    }
}

#[test]
fn present_metadata_as_cpp() {
    let edr = |a: f32, b: f32| WMTEDRValue {
        maximum_edr_color_component_value: a,
        maximum_potential_edr_color_component_value: b,
    };
    let bits = |m: DXMTPresentMetadata| {
        [
            m.edr_scale.to_bits(),
            m.max_content_luminance.to_bits(),
            m.max_display_luminance.to_bits(),
        ]
    };
    let f = |v: f32| v.to_bits();
    // SDR: the scale 1, 10000 nits of content, the display's potential × 100.
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_SRGB,
            edr(1.0, 1.0),
            Some(hdr("big"))
        )),
        [f(1.0), f(10000.0), f(100.0)]
    );
    // PQ: the EDR ratio; with metadata the largest of the display and the three levels.
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_HDR_PQ,
            edr(2.0, 8.0),
            None
        )),
        [f(0.25), f(10000.0), f(800.0)]
    );
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_HDR_PQ,
            edr(2.0, 8.0),
            Some(hdr("some"))
        )),
        [f(0.25), f(1000.0), f(800.0)]
    );
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_HDR_PQ,
            edr(2.0, 8.0),
            Some(hdr("big"))
        )),
        [f(0.25), f(9000.0), f(800.0)]
    );
    // scRGB: the ratio × the double 0.8, rounded to a float (0.5 × 0.8 = 0x3ecccccd).
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_HDR_SC_RGB,
            edr(3.0, 6.0),
            None
        )),
        [0x3ecc_cccd, f(10000.0), f(600.0)]
    );
    // An invalid colour space has the HDR bit.
    assert_eq!(
        bits(present_metadata(
            WMT_COLOR_SPACE_INVALID,
            edr(4.0, 5.0),
            None
        )),
        [f(0.8), f(10000.0), f(500.0)]
    );
}

#[test]
fn abi_structs_are_the_cpps() {
    // clang's layout of `dxmt::DXMTPresentMetadata` and `struct dxmt_presenter_callbacks`.
    assert_eq!(size_of::<DXMTPresentMetadata>(), 12);
    assert_eq!(offset_of!(DXMTPresentMetadata, edr_scale), 0);
    assert_eq!(offset_of!(DXMTPresentMetadata, max_content_luminance), 4);
    assert_eq!(offset_of!(DXMTPresentMetadata, max_display_luminance), 8);
    let ptr = size_of::<usize>();
    assert_eq!(size_of::<DxmtPresenterCallbacks>(), 3 * ptr);
    assert_eq!(offset_of!(DxmtPresenterCallbacks, wait), 0);
    assert_eq!(offset_of!(DxmtPresenterCallbacks, update), ptr);
    assert_eq!(offset_of!(DxmtPresenterCallbacks, ctx), 2 * ptr);
}

/// The golden's words of one op: `key=value` arguments, and `_<n>` for the n-th bare word.
fn args(words: &[&str]) -> Vec<(String, String)> {
    words
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, w)| {
            w.split_once('=').map_or_else(
                || (format!("_{i}"), (*w).to_owned()),
                |(k, v)| (k.to_owned(), v.to_owned()),
            )
        })
        .collect()
}

fn arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|a| a.0 == key).map(|a| a.1.as_str())
}

fn dims(s: &str) -> (u64, u64) {
    let (w, h) = s.split_once('x').expect("<w>x<h>");
    (w.parse().expect("a width"), h.parse().expect("a height"))
}

fn metadata_line(frame_id: u64, m: &DXMTPresentMetadata) -> String {
    let g9 = |v: f32| c_format(c"%.9g", f64::from(v));
    format!(
        "  -> frame_id={frame_id} metadata={{edr_scale={} max_content_luminance={} \
         max_display_luminance={}}} bits={:08x},{:08x},{:08x}",
        g9(m.edr_scale),
        g9(m.max_content_luminance),
        g9(m.max_display_luminance),
        m.edr_scale.to_bits(),
        m.max_content_luminance.to_bits(),
        m.max_display_luminance.to_bits()
    )
}

/// One scenario of the golden's matrix through the Rust presenter.
///
/// Printed as the C++ harness prints it.
fn scenario(line: &str, ramps: &[Box<DXMTGammaRamp>; 2]) {
    let (name, ops) = line.split_once('|').expect("<name> | <ops>");
    reset();
    say(&format!("== scenario {}", name.trim()));
    let mut presenter: Option<Presenter> = None;
    let mut pending: VecDeque<(u64, DXMTPresentMetadata)> = VecDeque::new();
    for op in ops.split(';') {
        let words: Vec<&str> = op.split_whitespace().collect();
        let Some(&verb) = words.first() else {
            continue;
        };
        let a = args(&words);
        say(&format!("> {}", op.trim()));
        if verb == "ctor" {
            let (mut width, mut height, mut scale) = (2560.0, 1440.0, 2.0);
            if let Some(layer) = arg(&a, "layer") {
                let (size, s) = layer.split_once('@').expect("<w>x<h>@<scale>");
                let (w, h) = dims(size);
                (width, height, scale) =
                    (u64_to_f64(w), u64_to_f64(h), s.parse().expect("a scale"));
            }
            fake().layer = layer_props(width, height, scale);
            let scale_factor = arg(&a, "scale").map_or(1.0, |s| s.parse().expect("a factor"));
            let samples = arg(&a, "samples").map_or(1, |s| s.parse().expect("a count"));
            presenter = Some(new_presenter(scale_factor, samples));
            continue;
        }
        let p = presenter.as_ref().expect("a ctor first");
        match verb {
            "props" => {
                let (w, h) = (arg(&a, "w").expect("w"), arg(&a, "h").expect("h"));
                let invalidated = p.change_layer_properties(
                    format(arg(&a, "fmt").expect("fmt")),
                    colorspace(arg(&a, "cs").expect("cs")),
                    u64_to_f64(w.parse().expect("a width")),
                    u64_to_f64(h.parse().expect("a height")),
                    arg(&a, "ms").expect("ms").parse().expect("a count"),
                );
                say(&format!("  -> invalidated={}", u8::from(invalidated)));
            }
            "cs" => {
                let invalidated =
                    p.change_layer_color_space(colorspace(arg(&a, "_1").expect("a name")));
                say(&format!("  -> invalidated={}", u8::from(invalidated)));
            }
            "hdr" => match arg(&a, "_1") {
                Some("none") => p.change_hdr_metadata(None),
                word => p.change_hdr_metadata(Some(&hdr(word.expect("a word")))),
            },
            "gamma" => match arg(&a, "_1") {
                Some("none") => p.change_gamma_ramp(None),
                word => p.change_gamma_ramp(Some(&ramps[usize::from(word != Some("v1"))])),
            },
            "display" => {
                let (max, potential) = arg(&a, "edr")
                    .expect("edr")
                    .split_once('/')
                    .expect("<a>/<b>");
                fake().display = (
                    arg(&a, "ver").expect("ver").parse().expect("a version"),
                    colorspace(arg(&a, "cs").expect("cs")),
                    hdr(arg(&a, "hdr").expect("hdr")),
                    WMTEDRValue {
                        maximum_edr_color_component_value: max.parse().expect("a value"),
                        maximum_potential_edr_color_component_value: potential
                            .parse()
                            .expect("a value"),
                    },
                );
            }
            "sync" => {
                let (frame_id, metadata) = p.synchronize();
                say(&metadata_line(frame_id, &metadata));
                pending.push_back((frame_id, metadata));
            }
            "syncwait" => {
                let got = thread::scope(|s| {
                    let queries = QUERIES.load(SeqCst);
                    let t = s.spawn(|| p.synchronize());
                    while QUERIES.load(SeqCst) == queries {
                        thread::yield_now();
                    }
                    for _ in 0..1000 {
                        if t.is_finished() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    let returned = if t.is_finished() {
                        "RETURNED"
                    } else {
                        "has not returned"
                    };
                    say(&format!(
                        "  -> the sync {returned} while the frames were still kept"
                    ));
                    for (frame_id, _) in core::mem::take(&mut pending) {
                        p.frame_done(frame_id);
                    }
                    t.join().expect("the sync")
                });
                say(&format!(
                    "  -> after the frames are done the sync returned, frame_id={}",
                    got.0
                ));
                pending.push_back(got);
            }
            "encode" => {
                fake().source = dims(arg(&a, "bb").expect("bb"));
                let fences = arg(&a, "fences").unwrap_or("none");
                let wait = |encoder| {
                    if fences == "wait" || fences == "both" {
                        fence(
                            encoder,
                            WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
                            0xF1,
                            WMT_RENDER_STAGE_VERTEX,
                        );
                    }
                };
                let update = |encoder| {
                    if fences == "update" || fences == "both" {
                        fence(
                            encoder,
                            WMT_RENDER_COMMAND_UPDATE_FENCE,
                            0xF2,
                            WMT_RENDER_STAGE_FRAGMENT,
                        );
                    }
                };
                let none = DXMTPresentMetadata {
                    edr_scale: 1.0,
                    max_content_luminance: 10000.0,
                    max_display_luminance: 1000.0,
                };
                let metadata = pending.back().map_or(&none, |f| &f.1);
                // SAFETY: the fakes' objects.
                let drawable = unsafe { p.encode(CMDBUF, BACKBUFFER, metadata, wait, update) };
                let name = fake().name(drawable.as_ref().map_or(0, Obj::handle));
                say(&format!("  -> drawable={name}"));
            }
            "done" => {
                if let Some((frame_id, _)) = pending.pop_front() {
                    p.frame_done(frame_id);
                }
            }
            "done-last" => {
                if let Some((frame_id, _)) = pending.pop_back() {
                    p.frame_done(frame_id);
                }
            }
            _ => say("  unknown op"),
        }
    }
    if let Some(p) = &presenter {
        for (frame_id, _) in core::mem::take(&mut pending) {
            p.frame_done(frame_id);
        }
    }
    say("-- destroy presenter");
}

/// What the golden's encode callbacks encode: one fence record.
fn fence(encoder: u64, kind: u16, fence: u64, stages: u8) {
    let record = WmtcmdRenderFenceOp {
        r#type: kind,
        reserved: [0; 3],
        next: mem(null::<u8>()),
        fence,
        stages,
    };
    // SAFETY: the fakes' encoder; a fence record, alive for the call.
    unsafe { MTLRenderCommandEncoder_encodeCommands(encoder, from_ref(&record).cast()) };
}

#[test]
fn golden_replays() {
    let _serial = serial();
    let ramps = [ramp(false), ramp(true)];
    let mut got = String::new();
    for line in GOLDEN_MATRIX.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        scenario(line, &ramps);
        got.push_str(&take());
    }
    // The retains and releases are the shader cache tests' fakes in this test binary; the C++
    // golden's harness, built against the Rust, pins them.
    let want: Vec<&str> = GOLDEN_CALLS
        .lines()
        .filter(|l| !l.starts_with("  NSObject_retain ") && !l.starts_with("  NSObject_release "))
        .collect();
    let got: Vec<&str> = got.lines().collect();
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "line {} of the filtered golden", i + 1);
    }
    assert_eq!(got.len(), want.len());
}
