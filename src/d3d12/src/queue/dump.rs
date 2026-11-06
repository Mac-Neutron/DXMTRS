//! Pass dumps, F9 frame dumps, pixel history, and Metal frame capture
//!
//! (`d3d12_command_queue.cpp:61-80, 730-1226, 1510-1516, 2128-2157`).

use core::{fmt::Write, ptr};
use std::sync::{LazyLock, Mutex};

use dxmt_core::{
    capture::{CaptureState, NextAction},
    format::texel_size,
};
use dxmt_util::{env, sync::lock};
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLCaptureManager_sharedCaptureManager,
    MTLCaptureManager_startCapture, MTLCaptureManager_stopCapture,
    MTLCommandBuffer_blitCommandEncoder, MTLCommandBuffer_renderCommandEncoder,
    MTLCommandEncoder_endEncoding, MTLCommandEncoder_setLabel, MTLDevice_newBuffer,
    MTLDevice_newTexture, MTLRenderCommandEncoder_encodeCommands, NSString_string, Obj,
    wire::{
        MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
        WMT_BLIT_OPTION_NONE, WMT_CAPTURE_DESTINATION_GPU_TRACE_DOCUMENT,
        WMT_LOAD_ACTION_DONT_CARE, WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_DRAW_INDEXED, WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
        WMT_RENDER_COMMAND_DRAW_INDIRECT, WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS,
        WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
        WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_NOP,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
        WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_PRE_RASTER, WMT_RENDER_STAGE_VERTEX,
        WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED,
        WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMTBufferInfo, WMTCaptureInfo, WMTOrigin, WMTRenderPassInfo,
        WMTRenderPassSampleBufferInfo, WMTSize, WMTTextureInfo, WMTUTF8_STRING_ENCODING,
        WmtcmdBase, WmtcmdBlitCopyFromBufferToBuffer,
        WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
        WmtcmdRenderSetpso,
    },
};

use crate::encoder::{BlitEncoderData, EncoderData, EncoderType, RenderEncoderData};

/// Win32 `VK_F9` virtual-key code (`0x78`).
pub const VK_F9: i32 = 0x78;

/// Win32 `VK_F10` virtual-key code (`0x79`).
pub const VK_F10: i32 = 0x79;

/// Single-frame attachment dump budget (`3 GiB`, `d3d12_command_queue.cpp:842`).
const SINGLE_FRAME_DUMP_BUDGET: u64 = 3 << 30;

/// Multi-frame burst attachment dump budget (`12 GiB`, `d3d12_command_queue.cpp:842`).
const BURST_FRAME_DUMP_BUDGET: u64 = 12 << 30;

/// Pass kind names written to `passes.txt` (`d3d12_command_queue.cpp:881`).
const PASS_KINDS: [&str; 7] = [
    "null", "clear", "render", "blit", "compute", "resolve", "upscale",
];

/// Blit command names written to `passes.txt` (`d3d12_command_queue.cpp:888-890`).
const BLIT_OPS: [&str; 14] = [
    "nop",
    "buf>buf",
    "buf>tex",
    "tex>buf",
    "tex>tex",
    "mips",
    "waitfence",
    "updatefence",
    "fill",
    "counters",
    "buf>tex+opt",
    "tex>buf+opt",
    "reset-icb",
    "copytex",
];

#[cfg(not(feature = "perf-build"))]
fn dump_on() -> bool {
    dxmt_core::dump::on()
}

#[cfg(feature = "perf-build")]
const fn dump_on() -> bool {
    false
}

#[cfg(not(feature = "perf-build"))]
fn save_capture(name: &str, data: &[u8]) {
    dxmt_core::dump::save(name, data);
}

#[cfg(feature = "perf-build")]
const fn save_capture(_name: &str, _data: &[u8]) {}

#[cfg(not(feature = "perf-build"))]
fn make_capture_folder(name: &str) -> bool {
    dxmt_core::dump::make_folder(name)
}

#[cfg(feature = "perf-build")]
const fn make_capture_folder(_name: &str) -> bool {
    true
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(vkey: i32) -> i16;
}

#[cfg(windows)]
fn key_down(vkey: i32) -> bool {
    // SAFETY: `GetAsyncKeyState` is a thread-safe Win32 syscall taking a virtual-key constant.
    let state = unsafe { GetAsyncKeyState(vkey) };
    state.cast_unsigned() & 0x8000 != 0
}

#[cfg(not(windows))]
const fn key_down(_vkey: i32) -> bool {
    false
}

#[cfg(windows)]
fn unix_path(path: &str) -> String {
    use core::ffi::{c_char, c_void};
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn GetProcessHeap() -> *mut c_void;
        fn HeapFree(heap: *mut c_void, flags: u32, mem: *mut c_void) -> i32;
        fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *mut c_void;
    }

    type WineGetUnixFileName = unsafe extern "C" fn(*const u16) -> *mut c_char;

    const KERNEL32: [u16; 13] = [
        b'k' as u16,
        b'e' as u16,
        b'r' as u16,
        b'n' as u16,
        b'e' as u16,
        b'l' as u16,
        b'3' as u16,
        b'2' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];
    // SAFETY: `KERNEL32` is NUL-terminated and `kernel32.dll` stays loaded for the process life.
    let kernel32 = unsafe { GetModuleHandleW(KERNEL32.as_ptr()) };
    if kernel32.is_null() {
        return String::new();
    }
    // SAFETY: `kernel32` is a live module handle and the symbol name is NUL-terminated.
    let proc =
        unsafe { GetProcAddress(kernel32, c"wine_get_unix_file_name".as_ptr().cast::<u8>()) };
    if proc.is_null() {
        return String::new();
    }
    // SAFETY: `wine_get_unix_file_name` has signature `extern "C" fn(*const u16) -> *mut c_char`.
    let get_unix_file_name: WineGetUnixFileName = unsafe { core::mem::transmute(proc) };
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(core::iter::once(0))
        .collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer valid for the duration of the call.
    let raw = unsafe { get_unix_file_name(wide.as_ptr()) };
    if raw.is_null() {
        return String::new();
    }
    // SAFETY: `raw` is a NUL-terminated C string allocated on the process heap.
    let out = unsafe { core::ffi::CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: `GetProcessHeap` returns the process heap handle.
    let heap = unsafe { GetProcessHeap() };
    // SAFETY: `raw` was allocated on `heap` by `wine_get_unix_file_name`.
    let _ = unsafe { HeapFree(heap, 0, raw.cast::<c_void>()) };
    out
}

#[cfg(not(windows))]
fn unix_path(path: &str) -> String {
    path.to_owned()
}

fn parse_leading_u64(s: &str) -> Option<u64> {
    let trimmed = s.trim_start();
    let digits_len = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    trimmed[..digits_len].parse::<u64>().ok()
}

fn parse_leading_u32(s: &str) -> Option<(u32, &str)> {
    let trimmed = s.trim_start();
    let digits_len = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    let val = trimmed[..digits_len].parse::<u32>().ok()?;
    Some((val, &trimmed[digits_len..]))
}

struct ParsedPixel {
    x: u32,
    y: u32,
    range: Option<(u32, u32)>,
    mode: Option<bool>,
}

/// Parses one `+`-separated token of `DXMT_DUMP_PIXEL` (`sscanf("%u,%u,%u,%u,%3s")`,
///
/// `d3d12_command_queue.cpp:804`).
fn parse_pixel_token(token: &str) -> Option<ParsedPixel> {
    let (x, rest) = parse_leading_u32(token)?;
    let rest = rest.strip_prefix(',')?;
    let (y, rest) = parse_leading_u32(rest)?;
    let Some(rest) = rest.strip_prefix(',') else {
        return Some(ParsedPixel {
            x,
            y,
            range: None,
            mode: None,
        });
    };
    let Some((first, rest)) = parse_leading_u32(rest) else {
        return Some(ParsedPixel {
            x,
            y,
            range: None,
            mode: None,
        });
    };
    let Some(rest) = rest.strip_prefix(',') else {
        return Some(ParsedPixel {
            x,
            y,
            range: None,
            mode: None,
        });
    };
    let Some((last, rest)) = parse_leading_u32(rest) else {
        return Some(ParsedPixel {
            x,
            y,
            range: None,
            mode: None,
        });
    };
    let mode = rest.strip_prefix(',').and_then(|after_comma| {
        let trimmed = after_comma.trim_start();
        let word: String = trimmed
            .chars()
            .take_while(|c| !c.is_ascii_whitespace())
            .take(3)
            .collect();
        (!word.is_empty()).then_some(word == "seq")
    });
    Some(ParsedPixel {
        x,
        y,
        range: Some((first, last)),
        mode,
    })
}

/// Process-wide pass-dump and pixel-history state (`d3d12_command_queue.cpp:760-823`).
pub struct Dumps {
    pub active: bool,
    pub frame: u32,
    pub pass: u32,
}

impl Dumps {
    /// Creates an inactive dump state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: false,
            frame: 0,
            pass: 0,
        }
    }
}

impl Default for Dumps {
    fn default() -> Self {
        Self::new()
    }
}

/// Staged attachment buffer queued to be written out by [`save_dumps`] (`PassDump`,
///
/// `d3d12_command_queue.cpp:739-744`).
pub struct PassDump {
    pub buffer: Obj,
    pub info: WMTBufferInfo,
    pub name: String,
}

/// Per-pass pixel-history readback record (`PassDumps::PixelPass`,
///
/// `d3d12_command_queue.cpp:766-774`).
pub struct PixelPass {
    pub pass: String,
    pub seq: bool,
    pub points: Vec<(u32, u32)>,
    pub draws: Vec<String>,
    pub colors: Vec<u32>,
    pub texels: Vec<u32>,
    pub buffer: Option<Obj>,
    pub info: WMTBufferInfo,
}

/// Inputs describing a render-pass attachment to dump (`DumpAttachment`,
///
/// `d3d12_command_queue.cpp:829-872`).
pub struct AttachmentSpec<'a> {
    pub name: &'a str,
    pub tex_width: u32,
    pub tex_height: u32,
    pub samples: u32,
    pub format: u32,
    pub level: u32,
    pub slice: u32,
    pub depth_plane: u32,
    pub option: u16,
    pub alloc_tex: u64,
}

/// Attachment metadata and `passes.txt` tag for a dumped render target or depth surface
///
/// (`DumpAttachment`, `d3d12_command_queue.cpp:829-872`).
#[derive(Debug, PartialEq, Eq)]
pub struct AttachmentMeta {
    pub file: String,
    pub tag: String,
    pub width: u64,
    pub height: u64,
    pub level: u32,
    pub slice: u32,
    pub texel_bytes: u32,
    pub size: u64,
}

/// Computes the `passes.txt` tag and staging dimensions for a render pass attachment
///
/// (`DumpAttachment`, `d3d12_command_queue.cpp:829-872`).
#[must_use]
pub fn format_attachment_meta(spec: &AttachmentSpec<'_>) -> AttachmentMeta {
    let width = u64::from(spec.tex_width.checked_shr(spec.level).unwrap_or(0).max(1));
    let height = u64::from(spec.tex_height.checked_shr(spec.level).unwrap_or(0).max(1));
    let texel_bytes = if spec.option != 0 {
        4
    } else {
        texel_size(spec.format)
    };
    let file = format!("{}-{width}x{height}-{}", spec.name, spec.format);
    let msaa = if spec.samples > 1 { "-msaa" } else { "" };
    let tag = format!(" {file}{msaa}@{}", spec.alloc_tex);
    let size = width * height * u64::from(texel_bytes);
    AttachmentMeta {
        file,
        tag,
        width,
        height,
        level: spec.level,
        slice: spec.slice,
        texel_bytes,
        size,
    }
}

/// Process-wide `PassDumps` state (`d3d12_command_queue.cpp:746-825`).
pub struct PassDumps {
    pub frames: u64,
    pub frame: u64,
    pub count: u64,
    pub on_key: bool,
    pub saved: u32,
    pub passes: u32,
    pub queues: u32,
    pub bytes: u64,
    pub dumps: Vec<PassDump>,
    pub log: String,
    pub pixel_on: bool,
    pub pixel_seq: bool,
    pub pixel_notes: String,
    pub pixel_draws: String,
    pub pixels: Vec<(u32, u32)>,
    pub pixel_first: u32,
    pub pixel_last: u32,
    pub pixel_passes: Vec<PixelPass>,
    pub pixel_textures: Vec<Obj>,
}

impl PassDumps {
    /// Creates an inactive `PassDumps` state (`d3d12_command_queue.cpp:748-776`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frames: 0,
            frame: u64::MAX,
            count: 1,
            on_key: false,
            saved: 0,
            passes: 0,
            queues: 0,
            bytes: 0,
            dumps: Vec::new(),
            log: String::new(),
            pixel_on: false,
            pixel_seq: false,
            pixel_notes: String::new(),
            pixel_draws: String::new(),
            pixels: Vec::new(),
            pixel_first: 0,
            pixel_last: u32::MAX,
            pixel_passes: Vec::new(),
            pixel_textures: Vec::new(),
        }
    }

    /// Creates a `PassDumps` state configured to dump `count` frames starting at `frame`.
    #[must_use]
    pub const fn for_range(frame: u64, count: u64) -> Self {
        let mut s = Self::new();
        s.frame = frame;
        s.count = if count > 0 { count } else { 1 };
        s
    }

    /// Creates a `PassDumps` state waiting for the F9 trigger key to dump `count` frames.
    #[must_use]
    pub const fn for_f9(count: u64) -> Self {
        let mut s = Self::new();
        s.on_key = true;
        s.count = if count > 0 { count } else { 1 };
        s
    }

    /// Parses `DXMT_DUMP_FRAME`, `DXMT_DUMP_FRAMES`, and `DXMT_DUMP_PIXEL` when capture mode is
    ///
    /// enabled (`PassDumps::PassDumps`, `d3d12_command_queue.cpp:784-819`).
    #[must_use]
    pub fn from_env_vars(
        capture_enabled: bool,
        dump_frame: Option<&str>,
        dump_frames: Option<&str>,
        dump_pixel: Option<&str>,
    ) -> Self {
        let mut d = Self::new();
        let value = dump_frame.unwrap_or_default();
        if value.is_empty() || !capture_enabled {
            return d;
        }
        d.on_key = value == "F9";
        if let Some(parsed_frame) = parse_leading_u64(value) {
            d.frame = parsed_frame;
        }
        if let Some(parsed_count) = dump_frames.and_then(parse_leading_u64) {
            d.count = parsed_count.max(1);
        }
        let spec = dump_pixel.unwrap_or_default();
        if !spec.is_empty() {
            for token in spec.split('+') {
                let Some(parsed) = parse_pixel_token(token) else {
                    break;
                };
                d.pixels.push((parsed.x, parsed.y));
                if let Some((first, last)) = parsed.range {
                    d.pixel_first = first;
                    d.pixel_last = last;
                }
                if let Some(seq) = parsed.mode {
                    d.pixel_seq = seq;
                }
            }
        }
        d.pixel_on = !d.pixels.is_empty();
        d
    }

    /// Reads `DXMT_DUMP_FRAME`, `DXMT_DUMP_FRAMES`, and `DXMT_DUMP_PIXEL` from the process
    ///
    /// environment (`d3d12_command_queue.cpp:784-819`).
    #[must_use]
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok();
        Self::from_env_vars(
            dump_on(),
            var("DXMT_DUMP_FRAME").as_deref(),
            var("DXMT_DUMP_FRAMES").as_deref(),
            var("DXMT_DUMP_PIXEL").as_deref(),
        )
    }

    /// Whether the frame now encoding is inside the active dump window (`PassDumps::Dumping`,
    ///
    /// `d3d12_command_queue.cpp:779-782`).
    #[must_use]
    pub const fn is_dumping(&self) -> bool {
        self.frame != u64::MAX && self.frames.wrapping_sub(self.frame) < self.count
    }

    /// Appends `# frame N presented` to `self.log` when dumping is active
    ///
    /// (`d3d12_command_queue.cpp:2127-2130`).
    pub fn note_present(&mut self) {
        if self.is_dumping() {
            let rel = self.frames.wrapping_sub(self.frame);
            let _ = writeln!(self.log, "# frame {rel} presented");
        }
    }

    /// Allocates the next `f9-<n>\` directory prefix using `make_dir` (`SaveDumps`,
    ///
    /// `d3d12_command_queue.cpp:1168-1171`).
    pub fn next_f9_dir<F: FnMut(&str) -> bool>(&mut self, mut make_dir: F) -> String {
        loop {
            self.saved = self.saved.wrapping_add(1);
            let dir = format!("f9-{}", self.saved);
            if make_dir(&dir) {
                return format!("{dir}\\");
            }
        }
    }

    /// Writes all staged attachments, pixel history logs, and `passes.txt` through `save_fn` and
    ///
    /// `make_dir` (`SaveDumps`, `d3d12_command_queue.cpp:1160-1219`).
    pub fn save_with<D, S>(&mut self, mut make_dir: D, mut save_fn: S)
    where
        D: FnMut(&str) -> bool,
        S: FnMut(&str, &[u8]),
    {
        if self.log.is_empty() {
            return;
        }
        let dir = if self.on_key {
            self.next_f9_dir(&mut make_dir)
        } else {
            String::new()
        };
        for dump in &self.dumps {
            let ptr = dump.info.memory.as_ptr::<u8>();
            let len = usize::try_from(dump.info.length).unwrap_or(0);
            if !ptr.is_null() && len > 0 {
                // SAFETY: `dump.buffer` is a live shared Metal buffer of `dump.info.length` bytes.
                let data = unsafe { core::slice::from_raw_parts(ptr, len) };
                save_fn(&format!("{dir}{}", dump.name), data);
            }
        }
        if self.pixel_on {
            let text = self.format_pixel_history();
            save_fn(&format!("{dir}pixels.txt"), text.as_bytes());
            save_fn(&format!("{dir}draws.txt"), self.pixel_draws.as_bytes());
            self.pixel_notes.clear();
            self.pixel_draws.clear();
            self.pixel_passes.clear();
            self.pixel_textures.clear();
        }
        save_fn(&format!("{dir}passes.txt"), self.log.as_bytes());
        self.dumps.clear();
        self.log.clear();
        self.passes = 0;
        self.bytes = 0;
        if self.on_key {
            self.frame = u64::MAX;
        }
    }

    /// Formats `pixels.txt` from the completed pixel-history readback buffers
    ///
    /// (`d3d12_command_queue.cpp:1178-1204`).
    #[must_use]
    pub fn format_pixel_history(&self) -> String {
        let mut text = self.pixel_notes.clone();
        for p in &self.pixel_passes {
            let ptr = p.info.memory.as_ptr::<u8>();
            let total_len = usize::try_from(p.info.length).unwrap_or(0);
            if ptr.is_null() || total_len == 0 {
                continue;
            }
            // SAFETY: `p.buffer` is a live shared Metal buffer of `p.info.length` bytes.
            let bytes = unsafe { core::slice::from_raw_parts(ptr, total_len) };
            let colors = p.colors.len();
            let stride = 16 * colors * p.points.len();
            for k in 1..=p.draws.len() {
                for (q, &(px, py)) in p.points.iter().enumerate() {
                    let mut changes = String::new();
                    for i in 0..colors {
                        let at = (q * colors + i) * 16;
                        let texel_len = usize::try_from(p.texels[i]).unwrap_or(0);
                        let without_off = (if p.seq { k - 1 } else { 0 }) * stride + at;
                        let with_off = k * stride + at;
                        let Some(without) = bytes.get(without_off..without_off + texel_len) else {
                            continue;
                        };
                        let Some(with) = bytes.get(with_off..with_off + texel_len) else {
                            continue;
                        };
                        if without != with {
                            let _ = write!(
                                changes,
                                " c{} {}->{}",
                                p.colors[i],
                                hex_bytes(without),
                                hex_bytes(with)
                            );
                        }
                    }
                    if !changes.is_empty() {
                        let _ = writeln!(
                            text,
                            "{} draw-{} {}{changes} at {px},{py}",
                            p.pass,
                            k - 1,
                            p.draws[k - 1]
                        );
                    }
                }
            }
        }
        text
    }

    /// Advances the frame counter on present and returns `true` when dumps reach their save point
    ///
    /// (`d3d12_command_queue.cpp:2127-2136`).
    pub fn step_present<F: FnMut(&str) -> bool>(
        &mut self,
        f9_down: bool,
        make_dir: &mut F,
    ) -> bool {
        self.note_present();
        self.frames = self.frames.wrapping_add(1);
        let frames = self.frames;
        if self.frame != u64::MAX
            && frames
                == self
                    .frame
                    .wrapping_add(self.count)
                    .wrapping_sub(1)
                    .wrapping_add(8)
        {
            if self.on_key {
                let _ = self.next_f9_dir(make_dir);
                self.frame = u64::MAX;
            }
            self.dumps.clear();
            self.log.clear();
            self.passes = 0;
            self.bytes = 0;
            return true;
        }
        if self.on_key && self.frame == u64::MAX && f9_down {
            self.frame = frames;
        }
        false
    }
}

impl Default for PassDumps {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Formats the Metal encoder label for Instruments when `DXMT_STATS=1` (`LabelPass`,
///
/// `d3d12_command_queue.cpp:61-80`).
#[must_use]
pub fn format_encoder_label(pass: u32, kind: &str, render: Option<&RenderEncoderData>) -> String {
    let mut name = format!("pass-{pass} {kind}");
    if let Some(render) = render {
        let mut colors = 0u32;
        let mut first_dims: Option<(u32, u32)> = None;
        for c in &render.colors {
            if let Some(view) = &c.attachment {
                if colors == 0 {
                    let info = view.descriptor.info();
                    let mip = view.key.first_mip();
                    first_dims = Some((
                        info.width.checked_shr(mip).unwrap_or(0).max(1),
                        info.height.checked_shr(mip).unwrap_or(0).max(1),
                    ));
                }
                colors += 1;
            }
        }
        if first_dims.is_none()
            && let Some(view) = &render.depth.attachment
        {
            let info = view.descriptor.info();
            let mip = view.key.first_mip();
            first_dims = Some((
                info.width.checked_shr(mip).unwrap_or(0).max(1),
                info.height.checked_shr(mip).unwrap_or(0).max(1),
            ));
        }
        let depth_suffix = if render.depth.attachment.is_some() {
            "+d"
        } else {
            ""
        };
        let _ = write!(name, " {colors}c{depth_suffix}");
        if let Some((w, h)) = first_dims {
            let _ = write!(name, " {w}x{h}");
        }
    }
    name
}

/// Sets the Metal encoder label on `encoder_handle` when `DXMT_STATS=1` is active (`LabelPass`,
///
/// `d3d12_command_queue.cpp:61-80`).
pub fn label_pass(encoder_handle: u64, pass: u32, kind: &str, render: Option<&RenderEncoderData>) {
    if !crate::stats::on() || encoder_handle == 0 {
        return;
    }
    let mut label = format_encoder_label(pass, kind, render);
    label.push('\0');
    // SAFETY: `label` is NUL-terminated and valid for the duration of the synchronous call.
    let ns_str = unsafe { NSString_string(label.as_ptr().cast(), WMTUTF8_STRING_ENCODING) };
    if ns_str != 0 {
        // SAFETY: `encoder_handle` and `ns_str` are live Objective-C handles.
        unsafe { MTLCommandEncoder_setLabel(encoder_handle, ns_str) };
    }
}

fn append_pass_details(out: &mut String, pass_name: &str, pass: &EncoderData) {
    if pass.kind == EncoderType::Blit {
        // SAFETY: `pass.kind == EncoderType::Blit` guarantees `pass` heads a `BlitEncoderData`.
        let data = unsafe { &*ptr::from_ref(pass).cast::<BlitEncoderData>() };
        if pass.timestamp_only {
            out.push_str(" timestamps");
        }
        if pass.query_resolve_only {
            out.push_str(" query-resolve");
        }
        let mut cmd = data.cmd_head.next.as_ptr::<WmtcmdBase>();
        while !cmd.is_null() {
            // SAFETY: `cmd` points to a command node inside the allocator arena.
            let header = unsafe { *cmd };
            let op_name = BLIT_OPS
                .get(usize::from(header.r#type))
                .copied()
                .unwrap_or("?");
            out.push(' ');
            out.push_str(op_name);
            if header.r#type == WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER {
                let copy_ptr = ptr::with_exposed_provenance::<WmtcmdBlitCopyFromBufferToBuffer>(
                    cmd.expose_provenance(),
                );
                // SAFETY: `header.r#type == WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER` guarantees
                // `copy_ptr` points to a `WmtcmdBlitCopyFromBufferToBuffer`.
                let copy_len = unsafe { ptr::read_unaligned(copy_ptr).copy_length };
                let _ = write!(out, ":{copy_len}");
            }
            cmd = header.next.as_ptr::<WmtcmdBase>();
        }
    } else if pass.kind == EncoderType::Render {
        // SAFETY: `pass.kind == EncoderType::Render` guarantees `pass` heads a `RenderEncoderData`.
        let data = unsafe { &*ptr::from_ref(pass).cast::<RenderEncoderData>() };
        for (i, c) in data.colors.iter().enumerate() {
            if let Some(view) = &c.attachment {
                let info = view.descriptor.info();
                let mip = view.key.first_mip();
                let meta = format_attachment_meta(&AttachmentSpec {
                    name: &format!("{pass_name}-c{i}"),
                    tex_width: info.width.checked_shr(mip).unwrap_or(0).max(1),
                    tex_height: info.height.checked_shr(mip).unwrap_or(0).max(1),
                    samples: info.sample_count(),
                    format: view.key.format(),
                    level: u32::from(c.level),
                    slice: u32::from(c.slice),
                    depth_plane: c.depth_plane,
                    option: WMT_BLIT_OPTION_NONE,
                    alloc_tex: view.allocation.texture(),
                });
                out.push_str(&meta.tag);
            }
        }
        if let Some(view) = &data.depth.attachment {
            let info = view.descriptor.info();
            let mip = view.key.first_mip();
            let format = view.key.format();
            let option = if format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
                WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
            } else {
                WMT_BLIT_OPTION_NONE
            };
            let meta = format_attachment_meta(&AttachmentSpec {
                name: &format!("{pass_name}-d"),
                tex_width: info.width.checked_shr(mip).unwrap_or(0).max(1),
                tex_height: info.height.checked_shr(mip).unwrap_or(0).max(1),
                samples: info.sample_count(),
                format,
                level: u32::from(data.depth.level),
                slice: u32::from(data.depth.slice),
                depth_plane: 0,
                option,
                alloc_tex: view.allocation.texture(),
            });
            out.push_str(&meta.tag);
        }
    }
}

/// Formats the `passes.txt` label for pass `index` (`PassLabel` / `DumpPass`,
///
/// `d3d12_command_queue.cpp:877-915`).
#[must_use]
pub fn pass_label(index: usize, encoder: &EncoderData) -> String {
    let pass_name = format!("pass-{index}");
    let kind_idx = encoder.kind as usize;
    let kind = PASS_KINDS.get(kind_idx).copied().unwrap_or("other");
    let mut out = format!("{pass_name} {kind}");
    append_pass_details(&mut out, &pass_name, encoder);
    out
}

/// Stages a single render-pass attachment into `d.dumps` (`DumpAttachment`,
///
/// `d3d12_command_queue.cpp:829-872`).
fn stage_attachment(
    d: &mut PassDumps,
    spec: &AttachmentSpec<'_>,
    meta: &AttachmentMeta,
    src_texture: u64,
    cmdbuf: Option<&Obj>,
    mtl_device: Option<&Obj>,
    fences: &mut Option<&mut dyn PixelHistoryFences>,
) {
    d.log.push_str(&meta.tag);
    let budget = if d.count > 1 {
        BURST_FRAME_DUMP_BUDGET
    } else {
        SINGLE_FRAME_DUMP_BUDGET
    };
    if meta.texel_bytes == 0 || spec.samples > 1 || d.bytes.saturating_add(meta.size) > budget {
        return;
    }
    let Some((cmdbuf, mtl_device)) = cmdbuf.zip(mtl_device) else {
        return;
    };
    let mut info = WMTBufferInfo {
        length: meta.size,
        options: WMT_RESOURCE_STORAGE_MODE_SHARED,
        memory: MemPtr::NULL,
        gpu_address: 0,
    };
    // SAFETY: `mtl_device` is a live `MTLDevice` handle and `info` is valid for the call.
    let buf_handle = unsafe { MTLDevice_newBuffer(mtl_device.handle(), &raw mut info) };
    // SAFETY: `MTLDevice_newBuffer` returns a +1 retained object or 0.
    let Some(buffer) = (unsafe { Obj::adopt(buf_handle) }) else {
        return;
    };
    if info.memory.as_ptr::<u8>().is_null() {
        return;
    }
    let cmd = WmtcmdBlitCopyFromTextureToBufferWithblitoption {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
        reserved: [0; 3],
        next: MemPtr::NULL,
        src: src_texture,
        slice: meta.slice,
        level: u16::try_from(meta.level).unwrap_or(0),
        options: spec.option,
        origin: WMTOrigin {
            x: 0,
            y: 0,
            z: u64::from(spec.depth_plane),
        },
        size: WMTSize {
            width: meta.width,
            height: meta.height,
            depth: 1,
        },
        dst: buffer.handle(),
        offset: 0,
        bytes_per_row: u32::try_from(meta.width * u64::from(meta.texel_bytes)).unwrap_or(0),
        bytes_per_image: u32::try_from(meta.size).unwrap_or(0),
    };
    // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` handle.
    let blit_handle = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf.handle()) };
    if blit_handle != 0 {
        let fence_slot = fences.as_mut().map(|f| f.join_blit(blit_handle));
        // SAFETY: `blit_handle` is a live blit encoder and `cmd` is a single NUL-terminated command node.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(
                blit_handle,
                (&raw const cmd).cast::<WmtcmdBase>(),
            );
        };
        if let (Some(f), Some(slot)) = (fences.as_mut(), fence_slot) {
            f.update_blit(blit_handle, slot);
        }
        // SAFETY: `blit_handle` has not ended encoding yet.
        unsafe { MTLCommandEncoder_endEncoding(blit_handle) };
    }
    d.bytes += meta.size;
    d.dumps.push(PassDump {
        buffer,
        info,
        name: format!("{}.raw", meta.file),
    });
}

/// Records `pass` into `d` (`DumpPass`, `d3d12_command_queue.cpp:877-915`).
pub fn dump_pass(
    d: &mut PassDumps,
    pass: &EncoderData,
    queue_index: u32,
    execute_calls: u64,
    cmdbuf: Option<&Obj>,
    mtl_device: Option<&Obj>,
) {
    dump_pass_fenced(
        d,
        pass,
        queue_index,
        execute_calls,
        cmdbuf,
        mtl_device,
        None,
    );
}

/// Records `pass` into `d` and orders any attachment-readback blits through `fences`
///
/// (`DumpPass`, `d3d12_command_queue.cpp:877-915`).
pub fn dump_pass_fenced(
    d: &mut PassDumps,
    pass: &EncoderData,
    queue_index: u32,
    execute_calls: u64,
    cmdbuf: Option<&Obj>,
    mtl_device: Option<&Obj>,
    mut fences: Option<&mut dyn PixelHistoryFences>,
) {
    let pass_idx = d.passes;
    d.passes = d.passes.wrapping_add(1);
    let pass_name = format!("pass-{pass_idx}");
    let kind_idx = pass.kind as usize;
    let kind = PASS_KINDS.get(kind_idx).copied().unwrap_or("other");
    let _ = write!(d.log, "{pass_name} q{queue_index} x{execute_calls} {kind}");
    if pass.kind == EncoderType::Blit {
        append_pass_details(&mut d.log, &pass_name, pass);
    } else if pass.kind == EncoderType::Render {
        // SAFETY: `pass.kind == EncoderType::Render` guarantees `pass` heads a `RenderEncoderData`.
        let data = unsafe { &*ptr::from_ref(pass).cast::<RenderEncoderData>() };
        for (i, c) in data.colors.iter().enumerate() {
            if let Some(view) = &c.attachment {
                let info = view.descriptor.info();
                let mip = view.key.first_mip();
                let spec = AttachmentSpec {
                    name: &format!("{pass_name}-c{i}"),
                    tex_width: info.width.checked_shr(mip).unwrap_or(0).max(1),
                    tex_height: info.height.checked_shr(mip).unwrap_or(0).max(1),
                    samples: info.sample_count(),
                    format: view.key.format(),
                    level: u32::from(c.level),
                    slice: u32::from(c.slice),
                    depth_plane: c.depth_plane,
                    option: WMT_BLIT_OPTION_NONE,
                    alloc_tex: view.allocation.texture(),
                };
                let meta = format_attachment_meta(&spec);
                stage_attachment(
                    d,
                    &spec,
                    &meta,
                    view.texture(),
                    cmdbuf,
                    mtl_device,
                    &mut fences,
                );
            }
        }
        if let Some(view) = &data.depth.attachment {
            let info = view.descriptor.info();
            let mip = view.key.first_mip();
            let format = view.key.format();
            let option = if format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
                WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
            } else {
                WMT_BLIT_OPTION_NONE
            };
            let spec = AttachmentSpec {
                name: &format!("{pass_name}-d"),
                tex_width: info.width.checked_shr(mip).unwrap_or(0).max(1),
                tex_height: info.height.checked_shr(mip).unwrap_or(0).max(1),
                samples: info.sample_count(),
                format,
                level: u32::from(data.depth.level),
                slice: u32::from(data.depth.slice),
                depth_plane: 0,
                option,
                alloc_tex: view.allocation.texture(),
            };
            let meta = format_attachment_meta(&spec);
            stage_attachment(
                d,
                &spec,
                &meta,
                view.texture(),
                cmdbuf,
                mtl_device,
                &mut fences,
            );
        }
    }
    d.log.push('\n');
}

const fn is_render_draw_command(cmd_type: u16) -> bool {
    matches!(
        cmd_type,
        WMT_RENDER_COMMAND_DRAW
            | WMT_RENDER_COMMAND_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS
            | WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE
            | WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER
    )
}

fn new_scratch_texture(
    d: &mut PassDumps,
    mtl_device: &Obj,
    format: u32,
    width: u64,
    height: u64,
) -> Option<Obj> {
    let usage = WMT_TEXTURE_USAGE_RENDER_TARGET | WMT_TEXTURE_USAGE_SHADER_READ;
    let bits = (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
        | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        | (usage << WMTTextureInfo::USAGE_SHIFT);
    let mut info = WMTTextureInfo {
        pixel_format: format,
        width: u32::try_from(width).unwrap_or(1),
        height: u32::try_from(height).unwrap_or(1),
        depth: 1,
        array_length: 1,
        bits,
        options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    // SAFETY: `mtl_device` is a live `MTLDevice` handle and `info` is valid for the call.
    let raw = unsafe { MTLDevice_newTexture(mtl_device.handle(), &raw mut info) };
    // SAFETY: `MTLDevice_newTexture` returns a +1 retained texture handle or 0.
    let tex = unsafe { Obj::adopt(raw) }?;
    // SAFETY: `tex` holds a valid handle; retain a second owner for `d.pixel_textures`.
    if let Some(kept) = unsafe { Obj::retain(tex.handle()) } {
        d.pixel_textures.push(kept);
    }
    Some(tex)
}

struct PendingColor {
    idx: usize,
    texture: u64,
    slice: u32,
    level: u32,
    plane: u32,
    width: u64,
    height: u64,
    format: u32,
}

struct WatchedColor {
    texture: u64,
    slice: u32,
    level: u32,
    plane: u32,
    scratch: Obj,
    before: Obj,
}

struct DepthCopy {
    texture: u64,
    slice: u32,
    level: u32,
    width: u64,
    height: u64,
    scratch: Obj,
    before: Obj,
}

struct TexCopy {
    src: u64,
    src_slice: u32,
    src_level: u32,
    src_origin: WMTOrigin,
    src_size: WMTSize,
    dst: u64,
    dst_origin: WMTOrigin,
}

fn encode_blit_copy_tex_to_tex(blit_handle: u64, copy: &TexCopy) {
    let cmd = WmtcmdBlitCopyFromTextureToTexture {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
        reserved: [0; 3],
        next: MemPtr::NULL,
        src: copy.src,
        src_slice: copy.src_slice,
        src_level: copy.src_level,
        src_origin: copy.src_origin,
        src_size: copy.src_size,
        dst: copy.dst,
        dst_slice: 0,
        dst_level: 0,
        dst_origin: copy.dst_origin,
    };
    // SAFETY: `blit_handle` is a live blit encoder and `cmd` is a single command node.
    unsafe {
        MTLBlitCommandEncoder_encodeCommands(blit_handle, (&raw const cmd).cast::<WmtcmdBase>());
    };
}

fn encode_blit_read_texel(
    blit_handle: u64,
    src: u64,
    x: u32,
    y: u32,
    texel: u32,
    dst_buffer: u64,
    offset: u64,
) {
    let cmd = WmtcmdBlitCopyFromTextureToBufferWithblitoption {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
        reserved: [0; 3],
        next: MemPtr::NULL,
        src,
        slice: 0,
        level: 0,
        options: WMT_BLIT_OPTION_NONE,
        origin: WMTOrigin {
            x: u64::from(x),
            y: u64::from(y),
            z: 0,
        },
        size: WMTSize {
            width: 1,
            height: 1,
            depth: 1,
        },
        dst: dst_buffer,
        offset,
        bytes_per_row: texel,
        bytes_per_image: texel,
    };
    // SAFETY: `blit_handle` is a live blit encoder and `cmd` is a single command node.
    unsafe {
        MTLBlitCommandEncoder_encodeCommands(blit_handle, (&raw const cmd).cast::<WmtcmdBase>());
    };
}

/// Callbacks supplied by the queue so [`pixel_history`] can join and update queue fences around
///
/// its scratch encoders (`d3d12_command_queue.cpp:1106-1148`).
pub trait PixelHistoryFences {
    /// Joins the frontier into `blit_encoder` and returns the fence slot to update at end.
    fn join_blit(&mut self, blit_encoder: u64) -> usize;

    /// Updates `fence_slot` on `blit_encoder`.
    fn update_blit(&mut self, blit_encoder: u64, fence_slot: usize);

    /// Joins the frontier into `render_encoder` at `stage` and returns the fence slot to update.
    fn join_render(&mut self, render_encoder: u64, stage: u8) -> usize;

    /// Updates `fence_slot` on `render_encoder` at `stage`.
    fn update_render(&mut self, render_encoder: u64, fence_slot: usize, stage: u8);
}

/// Encodes per-draw pixel-history redraws into scratch attachments before `data` executes
///
/// (`PixelHistory`, `d3d12_command_queue.cpp:920-1157`).
pub fn pixel_history(
    d: &mut PassDumps,
    cmdbuf: &Obj,
    mtl_device: &Obj,
    data: &mut RenderEncoderData,
    real: &WMTRenderPassInfo,
    fences: &mut dyn PixelHistoryFences,
) {
    let pass = d.passes;
    if !d.pixel_on || pass < d.pixel_first || pass > d.pixel_last {
        return;
    }
    let skip = |notes: &mut String, why: &str| {
        let _ = writeln!(notes, "# pass-{pass} not redrawn: {why}");
    };
    if data.render_target_array_length > 1 {
        skip(&mut d.pixel_notes, "layered");
        return;
    }
    let mut draws: Vec<*mut WmtcmdBase> = Vec::new();
    let mut visibility: Vec<*mut WmtcmdBase> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut pipeline = String::from("(no pipeline)");
    let mut cmd = (&raw mut data.cmd_head).cast::<WmtcmdBase>();
    while !cmd.is_null() {
        // SAFETY: `cmd` walks the intrusive command list starting at `data.cmd_head`.
        let header = unsafe { *cmd };
        let cmd_type = header.r#type;
        if cmd_type == WMT_RENDER_COMMAND_SET_PSO {
            let pso_ptr =
                ptr::with_exposed_provenance::<WmtcmdRenderSetpso>(cmd.expose_provenance());
            // SAFETY: `cmd_type == WMT_RENDER_COMMAND_SET_PSO` guarantees `WmtcmdRenderSetpso`.
            let pso = unsafe { ptr::read_unaligned(pso_ptr).pso };
            pipeline = crate::capture::pipeline_name(pso);
        } else if cmd_type == WMT_RENDER_COMMAND_SET_VISIBILITY_MODE {
            visibility.push(cmd);
        } else if is_render_draw_command(cmd_type) {
            draws.push(cmd);
            names.push(pipeline.clone());
        }
        cmd = header.next.as_ptr::<WmtcmdBase>();
    }
    if draws.is_empty() {
        return;
    }
    for (k, name) in names.iter().enumerate() {
        let _ = writeln!(d.pixel_draws, "pass-{pass} draw-{k} {name}");
    }

    let mut info = *real;
    info.visibility_buffer = 0;
    info.visibility_accumulate = 0;
    info.sample_buffers = [WMTRenderPassSampleBufferInfo {
        sample_buffer: 0,
        end_of_fragment_sample_index: 0,
        start_of_vertex_sample_index: 0,
    }; 4];

    let mut min_width = u64::MAX;
    let mut min_height = u64::MAX;
    let mut pending_colors: Vec<PendingColor> = Vec::new();
    let mut record = PixelPass {
        pass: format!("pass-{pass}"),
        seq: d.pixel_seq,
        points: Vec::new(),
        draws: names,
        colors: Vec::new(),
        texels: Vec::new(),
        buffer: None,
        info: WMTBufferInfo {
            length: 0,
            options: 0,
            memory: MemPtr::NULL,
            gpu_address: 0,
        },
    };

    for (i, c) in data.colors.iter().enumerate() {
        let Some(view) = &c.attachment else {
            continue;
        };
        let tex_info = view.descriptor.info();
        if tex_info.sample_count() > 1 {
            skip(&mut d.pixel_notes, "multisampled");
            return;
        }
        let mip = view.key.first_mip() + u32::from(c.level);
        let width = u64::from(tex_info.width.checked_shr(mip).unwrap_or(0).max(1));
        let height = u64::from(tex_info.height.checked_shr(mip).unwrap_or(0).max(1));
        min_width = min_width.min(width);
        min_height = min_height.min(height);
        let format = view.key.format();
        pending_colors.push(PendingColor {
            idx: i,
            texture: view.texture(),
            slice: u32::from(c.slice),
            level: u32::from(c.level),
            plane: c.depth_plane,
            width,
            height,
            format,
        });
        record.colors.push(u32::try_from(i).unwrap_or(0));
        record.texels.push(texel_size(format));
    }
    if pending_colors.is_empty() {
        skip(&mut d.pixel_notes, "no color attachment");
        return;
    }
    for &(px, py) in &d.pixels {
        if u64::from(px) < min_width && u64::from(py) < min_height {
            record.points.push((px, py));
        }
    }
    if record.points.is_empty() {
        skip(&mut d.pixel_notes, "the pixels lie outside");
        return;
    }

    let points_len = u64::try_from(record.points.len()).unwrap_or(1);
    let mut colors: Vec<WatchedColor> = Vec::with_capacity(pending_colors.len());
    for pc in &pending_colors {
        let Some(scratch) = new_scratch_texture(d, mtl_device, pc.format, pc.width, pc.height)
        else {
            return;
        };
        let Some(before) = new_scratch_texture(d, mtl_device, pc.format, points_len, 1) else {
            return;
        };
        let color_slot = &mut info.colors[pc.idx];
        color_slot.texture = scratch.handle();
        color_slot.level = 0;
        color_slot.slice = 0;
        color_slot.depth_plane = 0;
        color_slot.resolve_texture = 0;
        color_slot.store_action = WMT_STORE_ACTION_STORE;
        if color_slot.load_action == WMT_LOAD_ACTION_DONT_CARE {
            color_slot.load_action = WMT_LOAD_ACTION_LOAD;
        }
        colors.push(WatchedColor {
            texture: pc.texture,
            slice: pc.slice,
            level: pc.level,
            plane: pc.plane,
            scratch,
            before,
        });
    }

    if let Some(stencil_view) = &data.stencil.attachment {
        let same_as_depth = data
            .depth
            .attachment
            .as_ref()
            .is_some_and(|dv| dv.texture() == stencil_view.texture());
        if !same_as_depth {
            skip(&mut d.pixel_notes, "separate stencil texture");
            return;
        }
    }

    let depth_copy = if let Some(depth_view) = &data.depth.attachment {
        let tex_info = depth_view.descriptor.info();
        let depth_slice = u32::from(data.depth.slice);
        let depth_level = u32::from(data.depth.level);
        let mip = depth_view.key.first_mip() + depth_level;
        let dw = u64::from(tex_info.width.checked_shr(mip).unwrap_or(0).max(1));
        let dh = u64::from(tex_info.height.checked_shr(mip).unwrap_or(0).max(1));
        let dfmt = depth_view.key.format();
        let Some(d_scratch) = new_scratch_texture(d, mtl_device, dfmt, dw, dh) else {
            return;
        };
        let Some(d_before) = new_scratch_texture(d, mtl_device, dfmt, dw, dh) else {
            return;
        };
        if info.depth.texture != 0 {
            info.depth.texture = d_scratch.handle();
            info.depth.level = 0;
            info.depth.slice = 0;
            info.depth.depth_plane = 0;
            info.depth.store_action = WMT_STORE_ACTION_STORE;
            if info.depth.load_action == WMT_LOAD_ACTION_DONT_CARE {
                info.depth.load_action = WMT_LOAD_ACTION_LOAD;
            }
        }
        if info.stencil.texture != 0 {
            info.stencil.texture = d_scratch.handle();
            info.stencil.level = 0;
            info.stencil.slice = 0;
            info.stencil.depth_plane = 0;
            info.stencil.store_action = WMT_STORE_ACTION_STORE;
            if info.stencil.load_action == WMT_LOAD_ACTION_DONT_CARE {
                info.stencil.load_action = WMT_LOAD_ACTION_LOAD;
            }
        }
        Some(DepthCopy {
            texture: depth_view.texture(),
            slice: depth_slice,
            level: depth_level,
            width: dw,
            height: dh,
            scratch: d_scratch,
            before: d_before,
        })
    } else {
        None
    };

    let slot_bytes: u64 = 16;
    let stride = slot_bytes
        * u64::try_from(colors.len()).unwrap_or(0)
        * u64::try_from(record.points.len()).unwrap_or(0);
    record.info.length = stride * u64::try_from(draws.len() + 1).unwrap_or(0);
    record.info.options = WMT_RESOURCE_STORAGE_MODE_SHARED;
    record.info.memory = MemPtr::NULL;
    // SAFETY: `mtl_device` is a live `MTLDevice` handle and `record.info` is valid for the call.
    let buf_raw = unsafe { MTLDevice_newBuffer(mtl_device.handle(), &raw mut record.info) };
    // SAFETY: `MTLDevice_newBuffer` returns a +1 retained buffer or 0.
    let Some(readback_buf) = (unsafe { Obj::adopt(buf_raw) }) else {
        skip(&mut d.pixel_notes, "no readback buffer");
        return;
    };
    if record.info.memory.as_ptr::<u8>().is_null() {
        skip(&mut d.pixel_notes, "no readback buffer");
        return;
    }
    let readback_handle = readback_buf.handle();
    record.buffer = Some(readback_buf);

    // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` handle.
    let before_blit_raw = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf.handle()) };
    if before_blit_raw != 0 {
        let fence = fences.join_blit(before_blit_raw);
        for w in &colors {
            for (p_idx, &(px, py)) in record.points.iter().enumerate() {
                encode_blit_copy_tex_to_tex(
                    before_blit_raw,
                    &TexCopy {
                        src: w.texture,
                        src_slice: w.slice,
                        src_level: w.level,
                        src_origin: WMTOrigin {
                            x: u64::from(px),
                            y: u64::from(py),
                            z: u64::from(w.plane),
                        },
                        src_size: WMTSize {
                            width: 1,
                            height: 1,
                            depth: 1,
                        },
                        dst: w.before.handle(),
                        dst_origin: WMTOrigin {
                            x: u64::try_from(p_idx).unwrap_or(0),
                            y: 0,
                            z: 0,
                        },
                    },
                );
            }
        }
        if let Some(ref dc) = depth_copy {
            encode_blit_copy_tex_to_tex(
                before_blit_raw,
                &TexCopy {
                    src: dc.texture,
                    src_slice: dc.slice,
                    src_level: dc.level,
                    src_origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    src_size: WMTSize {
                        width: dc.width,
                        height: dc.height,
                        depth: 1,
                    },
                    dst: dc.before.handle(),
                    dst_origin: WMTOrigin { x: 0, y: 0, z: 0 },
                },
            );
        }
        fences.update_blit(before_blit_raw, fence);
        // SAFETY: `before_blit_raw` has not ended encoding yet.
        unsafe { MTLCommandEncoder_endEncoding(before_blit_raw) };
    }

    let mut saved_types: Vec<u16> = Vec::with_capacity(draws.len());
    for &draw_cmd in &draws {
        // SAFETY: `draw_cmd` points to a valid `WmtcmdBase` in `data`.
        saved_types.push(unsafe { (*draw_cmd).r#type });
    }
    let nop_type = WMT_RENDER_COMMAND_NOP;
    for &vis_cmd in &visibility {
        // SAFETY: `vis_cmd` points to a valid `WmtcmdBase` in `data`.
        unsafe { (*vis_cmd).r#type = nop_type };
    }

    let join_stage = if data.use_geometry {
        WMT_RENDER_STAGE_PRE_RASTER
    } else {
        WMT_RENDER_STAGE_VERTEX
    };
    let frag_stage = WMT_RENDER_STAGE_FRAGMENT;

    for k in 0..=draws.len() {
        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` handle.
        let restore_raw = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf.handle()) };
        if restore_raw != 0 {
            let restored = fences.join_blit(restore_raw);
            for w in &colors {
                for (p_idx, &(px, py)) in record.points.iter().enumerate() {
                    encode_blit_copy_tex_to_tex(
                        restore_raw,
                        &TexCopy {
                            src: w.before.handle(),
                            src_slice: 0,
                            src_level: 0,
                            src_origin: WMTOrigin {
                                x: u64::try_from(p_idx).unwrap_or(0),
                                y: 0,
                                z: 0,
                            },
                            src_size: WMTSize {
                                width: 1,
                                height: 1,
                                depth: 1,
                            },
                            dst: w.scratch.handle(),
                            dst_origin: WMTOrigin {
                                x: u64::from(px),
                                y: u64::from(py),
                                z: 0,
                            },
                        },
                    );
                }
            }
            if let Some(ref dc) = depth_copy {
                encode_blit_copy_tex_to_tex(
                    restore_raw,
                    &TexCopy {
                        src: dc.before.handle(),
                        src_slice: 0,
                        src_level: 0,
                        src_origin: WMTOrigin { x: 0, y: 0, z: 0 },
                        src_size: WMTSize {
                            width: dc.width,
                            height: dc.height,
                            depth: 1,
                        },
                        dst: dc.scratch.handle(),
                        dst_origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    },
                );
            }
            fences.update_blit(restore_raw, restored);
            // SAFETY: `restore_raw` has not ended encoding yet.
            unsafe { MTLCommandEncoder_endEncoding(restore_raw) };
        }

        for (j, &draw_cmd) in draws.iter().enumerate() {
            let active = if record.seq { j < k } else { j + 1 == k };
            let new_type = if active { saved_types[j] } else { nop_type };
            // SAFETY: `draw_cmd` points to a valid `WmtcmdBase` in `data`.
            unsafe { (*draw_cmd).r#type = new_type };
        }

        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` and `info` is valid for the call.
        let render_raw =
            unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf.handle(), &raw mut info) };
        if render_raw != 0 {
            let render_fence = fences.join_render(render_raw, join_stage);
            // SAFETY: `render_raw` is a live render encoder and `data.cmd_head` heads a valid chain.
            unsafe {
                MTLRenderCommandEncoder_encodeCommands(
                    render_raw,
                    (&raw const data.cmd_head).cast::<WmtcmdBase>(),
                );
            };
            fences.update_render(render_raw, render_fence, frag_stage);
            // SAFETY: `render_raw` has not ended encoding yet.
            unsafe { MTLCommandEncoder_endEncoding(render_raw) };
        }

        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` handle.
        let readback_raw = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf.handle()) };
        if readback_raw != 0 {
            let copied = fences.join_blit(readback_raw);
            for (p_idx, &(px, py)) in record.points.iter().enumerate() {
                for (i, w) in colors.iter().enumerate() {
                    let offset = u64::try_from(k).unwrap_or(0) * stride
                        + u64::try_from(p_idx * colors.len() + i).unwrap_or(0) * slot_bytes;
                    encode_blit_read_texel(
                        readback_raw,
                        w.scratch.handle(),
                        px,
                        py,
                        record.texels[i],
                        readback_handle,
                        offset,
                    );
                }
            }
            fences.update_blit(readback_raw, copied);
            // SAFETY: `readback_raw` has not ended encoding yet.
            unsafe { MTLCommandEncoder_endEncoding(readback_raw) };
        }
    }

    for (j, &draw_cmd) in draws.iter().enumerate() {
        // SAFETY: `draw_cmd` points to a valid `WmtcmdBase` in `data`.
        unsafe { (*draw_cmd).r#type = saved_types[j] };
    }
    let vis_type = WMT_RENDER_COMMAND_SET_VISIBILITY_MODE;
    for &vis_cmd in &visibility {
        // SAFETY: `vis_cmd` points to a valid `WmtcmdBase` in `data`.
        unsafe { (*vis_cmd).r#type = vis_type };
    }

    let mode_desc = if record.seq { "in sequence" } else { "alone" };
    let _ = writeln!(
        d.pixel_notes,
        "# {}: {} draws redrawn {mode_desc}",
        record.pass,
        draws.len()
    );
    d.pixel_passes.push(record);
}

/// Frame-counted Metal capture schedule (`d3d12_command_queue.cpp:1510-1516, 2137-2157`).
pub struct CaptureSchedule {
    state: CaptureState,
    presented_frames: usize,
}

impl CaptureSchedule {
    /// Initializes an enabled capture schedule from an optional `DXMT_CAPTURE_FRAME` string.
    #[must_use]
    pub fn with_env(env_val: Option<&str>) -> Self {
        Self::new(CaptureState::new(true), env_val)
    }

    /// Initializes capture scheduling from the process environment (`MTL_CAPTURE_ENABLED`,
    ///
    /// `DXMT_CAPTURE_EXECUTABLE`, and `DXMT_CAPTURE_FRAME`, `d3d12_command_queue.cpp:1510-1516`).
    #[must_use]
    pub fn from_env() -> Self {
        let frame_var = std::env::var("DXMT_CAPTURE_FRAME").ok();
        Self::new(CaptureState::from_env(), frame_var.as_deref())
    }

    fn new(mut state: CaptureState, env_val: Option<&str>) -> Self {
        if let Some(frame) = env_val.and_then(parse_leading_u64) {
            state.schedule(frame);
        }
        Self {
            state,
            presented_frames: 0,
        }
    }

    /// Returns the number of presented frames observed so far.
    #[must_use]
    pub const fn presented_frames(&self) -> usize {
        self.presented_frames
    }

    /// Advances the capture state machine at present (`d3d12_command_queue.cpp:2137-2157`).
    pub fn step_present(&mut self, f10_down: bool) -> NextAction {
        self.presented_frames = self.presented_frames.wrapping_add(1);
        let cur = u64::try_from(self.presented_frames).unwrap_or(u64::MAX);
        let action = self.state.next_action(cur);
        if action == NextAction::Nothing && self.state.should_capture_next(f10_down) {
            self.state.schedule(cur.saturating_add(1));
        }
        action
    }
}

fn gputrace_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned();
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (if month <= 2 { y + 1 } else { y }).rem_euclid(100);
    format!("_{hour:02}'{min:02}'{sec:02}_{month:02}-{day:02}-{year:02}.gputrace")
}

static PASS_DUMPS: LazyLock<Mutex<PassDumps>> = LazyLock::new(|| Mutex::new(PassDumps::from_env()));
static CAPTURE_SCHEDULE: LazyLock<Mutex<CaptureSchedule>> =
    LazyLock::new(|| Mutex::new(CaptureSchedule::from_env()));

fn capture_enabled() -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        std::env::var_os("MTL_CAPTURE_ENABLED").is_some()
            || std::env::var_os("DXMT_CAPTURE_FRAME").is_some()
    });
    *ENABLED
}

/// Allocates the next process-wide queue index (`Dumps().queues++`,
///
/// `d3d12_command_queue.cpp:826`).
#[must_use]
pub fn next_queue_index() -> u32 {
    let mut d = lock(&PASS_DUMPS);
    let idx = d.queues;
    d.queues = d.queues.wrapping_add(1);
    idx
}

/// Returns whether the process-wide `PassDumps` state is currently inside the dump window
///
/// (`Dumps().Dumping()`, `d3d12_command_queue.cpp:779-782`).
#[must_use]
pub fn is_dumping() -> bool {
    lock(&PASS_DUMPS).is_dumping()
}

/// Runs `f` with exclusive access to the process-wide [`PassDumps`] instance.
pub fn with_pass_dumps<R, F: FnOnce(&mut PassDumps) -> R>(f: F) -> R {
    let mut d = lock(&PASS_DUMPS);
    f(&mut d)
}

/// Saves any pending pass dumps to the capture directory (`SaveDumps`,
///
/// `d3d12_command_queue.cpp:1160-1219, 1526`).
pub fn save_dumps() {
    let mut d = lock(&PASS_DUMPS);
    d.save_with(make_capture_folder, save_capture);
}

/// Advances F9 pass dumps and Metal frame capture at the end of a present
///
/// (`d3d12_command_queue.cpp:2128-2157`).
pub fn on_present(mtl_device: &Obj) {
    {
        let mut d = lock(&PASS_DUMPS);
        d.note_present();
        d.frames = d.frames.wrapping_add(1);
        let frames = d.frames;
        crate::stat_frame!();
        if d.frame != u64::MAX
            && frames
                == d.frame
                    .wrapping_add(d.count)
                    .wrapping_sub(1)
                    .wrapping_add(8)
        {
            d.save_with(make_capture_folder, save_capture);
        } else if d.on_key && d.frame == u64::MAX && key_down(VK_F9) {
            d.frame = frames;
        }
    }

    let (action, presented_frames) = if capture_enabled() {
        let mut sched = lock(&CAPTURE_SCHEDULE);
        let action = sched.step_present(key_down(VK_F10));
        (action, sched.presented_frames())
    } else {
        (NextAction::Nothing, 0)
    };
    match action {
        NextAction::StartCapture => {
            let raw_name = format!(
                "{}_F.{}{}",
                env::exe_base_name(),
                presented_frames.saturating_add(1),
                gputrace_stamp()
            );
            let file = unix_path(&raw_name);
            crate::warn!("A new capture will be saved to {file}");
            let mut file_c = file;
            file_c.push('\0');
            let mut info = WMTCaptureInfo {
                capture_object: mtl_device.handle(),
                output_url: MemPtr::from_ptr(file_c.as_ptr()),
                destination: WMT_CAPTURE_DESTINATION_GPU_TRACE_DOCUMENT,
            };
            // SAFETY: `MTLCaptureManager_sharedCaptureManager` has no preconditions.
            let mgr = unsafe { MTLCaptureManager_sharedCaptureManager() };
            if mgr != 0 {
                // SAFETY: `mgr` is valid and `info` / `file_c` outlive the synchronous call.
                let _ = unsafe { MTLCaptureManager_startCapture(mgr, &raw mut info) };
            }
        }
        NextAction::StopCapture => {
            // SAFETY: `MTLCaptureManager_sharedCaptureManager` has no preconditions.
            let mgr = unsafe { MTLCaptureManager_sharedCaptureManager() };
            if mgr != 0 {
                // SAFETY: `mgr` is a valid shared capture manager handle.
                unsafe { MTLCaptureManager_stopCapture(mgr) };
            }
        }
        NextAction::Nothing => {}
    }
}

#[cfg(test)]
mod tests;
