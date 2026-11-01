//! `DXMT_WMTCMD_HISTOGRAM` and `DXMT_WMTCMD_DUMP`: per-commit counts and per-record dumps.
//!
//! With `DXMT_WMTCMD_HISTOGRAM` naming a file, every `MTLCommandBuffer_commit` appends one line to
//! it for each (encoder kind, record type) counted since the previous commit, kinds in the order
//! `render`, `compute`, `blit` and types ascending:
//!
//! ```text
//! <kind> <record type, decimal> <count>
//! ```
//!
//! With `DXMT_WMTCMD_DUMP` naming a file, each `encodeCommands` call appends one line per record
//! (`<kind> <type> <field>=<value>…`), handles renumbered by first use (`#1`, `#2`, …, `0` for
//! `0`), GPU virtual addresses as `buf#<id>+<offset>` from the buffer range map, raw pointers as
//! `<len>B` (with contents on `set_bytes`), and floats as `f32::to_bits` hex. Unset, each knob
//! costs one test of a latched flag per `encodeCommands` call.

use std::{
    collections::BTreeMap,
    env,
    fs::{File, OpenOptions},
    io::Write,
    sync::{LazyLock, Mutex, PoisonError},
};

use winemetal_wire::{
    MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_BLIT_COMMAND_COPY_TEXTURE,
    WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
    WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER, WMT_BLIT_COMMAND_RESOLVE_COUNTERS,
    WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_COMPUTE_COMMAND_DISPATCH,
    WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
    WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_COMPUTE_COMMAND_MEMORY_BARRIER,
    WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
    WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE,
    WMT_COMPUTE_COMMAND_UPDATE_FENCE, WMT_COMPUTE_COMMAND_USE_RESOURCE,
    WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE, WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
    WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS, WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_MEMORY_BARRIER,
    WMT_RENDER_COMMAND_SET_BLEND_FACTOR, WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_DSSO,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_MESH_BUFFER,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
    WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_PSO,
    WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
    WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_VERTEX_BYTES, WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE, WMT_RENDER_COMMAND_SET_VIEWPORT,
    WMT_RENDER_COMMAND_SET_VIEWPORTS, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
    WMT_RENDER_COMMAND_UPDATE_FENCE, WMT_RENDER_COMMAND_USE_RESOURCE,
    WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WMTOrigin, WMTScissorRect, WMTSize, WMTViewport, WmtcmdBase,
    WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitCopyFromBufferToTextureWithblitoption, WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
    WmtcmdBlitCopyTexture, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdBlitGenerateMipmaps,
    WmtcmdBlitResetcommands, WmtcmdBlitResolvecounters, WmtcmdComputeDispatch,
    WmtcmdComputeDispatchIndirect, WmtcmdComputeExecutecommands, WmtcmdComputeFenceOp,
    WmtcmdComputeMemoryBarrier, WmtcmdComputeSetbuffer, WmtcmdComputeSetbufferoffset,
    WmtcmdComputeSetbytes, WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdComputeUseresource,
    WmtcmdRenderDispatchThreadsPerTile, WmtcmdRenderDraw, WmtcmdRenderDrawIndexed,
    WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect, WmtcmdRenderDrawMeshthreadgroups,
    WmtcmdRenderDrawMeshthreadgroupsIndirect, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
    WmtcmdRenderDxmtTessellationMeshDrawIndirect, WmtcmdRenderExecutecommands, WmtcmdRenderFenceOp,
    WmtcmdRenderMemoryBarrier, WmtcmdRenderSetblendcolor, WmtcmdRenderSetbuffer,
    WmtcmdRenderSetbufferoffset, WmtcmdRenderSetbytes, WmtcmdRenderSetdepthstencilstate,
    WmtcmdRenderSetdsso, WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate,
    WmtcmdRenderSetsampler, WmtcmdRenderSetscissorrect, WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetstencilref, WmtcmdRenderSettexture, WmtcmdRenderSetviewport,
    WmtcmdRenderSetviewports, WmtcmdRenderSetvisibilitymode, WmtcmdRenderUseresource,
};

use crate::{
    ffi::{mem_ref, mem_slice},
    float_cast::{f64_to_f32, u64_to_usize},
};

/// The histogram knob: a file name, appended to.
const VARIABLE: &str = "DXMT_WMTCMD_HISTOGRAM";

/// The record-dump knob: a file name, appended to.
const DUMP_VARIABLE: &str = "DXMT_WMTCMD_DUMP";

/// The file, open for the process's life: `None` when the knob is unset, empty or unopenable.
///
/// A static because the file is process-wide and latched once from the environment, and the
/// commit and encode handlers reach it without an object (CONVENTIONS, State lives on an object).
static HISTOGRAM: LazyLock<Option<Mutex<Histogram<File>>>> = LazyLock::new(|| {
    let name = env::var_os(VARIABLE).filter(|n| !n.is_empty())?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(name)
        .ok()?;
    Some(Mutex::new(Histogram::new(file)))
});

/// The per-record dump state, open for the process's life: `None` when `DXMT_WMTCMD_DUMP` is off.
///
/// A static for the same reason as [`HISTOGRAM`]: process-wide, latched once from the environment,
/// and reached by buffer creation and command encoding without a shared state object.
static DUMP: LazyLock<Option<Mutex<Dump<File>>>> = LazyLock::new(|| {
    let name = env::var_os(DUMP_VARIABLE).filter(|n| !n.is_empty())?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(name)
        .ok()?;
    Some(Mutex::new(Dump::new(file)))
});

/// The three command-list encoders whose records are counted.
pub enum Kind {
    /// `_MTLRenderCommandEncoder_encodeCommands`.
    Render,
    /// `_MTLComputeCommandEncoder_encodeCommands`.
    Compute,
    /// `_MTLBlitCommandEncoder_encodeCommands`.
    Blit,
}

/// The word a line starts with, by kind: the order lines are written in.
const WORDS: [&str; 3] = ["render", "compute", "blit"];

impl Kind {
    /// The kind's place in [`WORDS`].
    const fn index(&self) -> usize {
        match self {
            Self::Render => 0,
            Self::Compute => 1,
            Self::Blit => 2,
        }
    }
}

type Counts = BTreeMap<u16, u64>;

/// The counts since the last commit, and where a commit writes them.
pub struct Histogram<W: Write> {
    out: W,
    counts: [Counts; 3],
}

impl<W: Write> Histogram<W> {
    /// An empty histogram writing to `out`.
    #[must_use]
    pub const fn new(out: W) -> Self {
        Self {
            out,
            counts: [Counts::new(), Counts::new(), Counts::new()],
        }
    }

    /// Takes a finished list's counts in.
    pub fn add(&mut self, list: List) {
        let counts = &mut self.counts[list.kind.index()];
        for (record, n) in list.counts {
            *counts.entry(record).or_insert(0) += n;
        }
    }

    /// Appends a line per count since the last commit, in one write, and starts counting again.
    ///
    /// A write that fails loses the lines: this is a development aid, not a record.
    pub fn commit(&mut self) {
        let mut lines = Vec::new();
        for (word, counts) in WORDS.iter().zip(core::mem::take(&mut self.counts)) {
            for (record, n) in counts {
                let _ = writeln!(lines, "{word} {record} {n}");
            }
        }
        if !lines.is_empty() {
            let _ = self.out.write_all(&lines);
        }
    }
}

/// The records one `encodeCommands` call walked.
pub struct List {
    kind: Kind,
    counts: Counts,
}

impl List {
    /// An empty list of `kind`.
    #[must_use]
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            counts: Counts::new(),
        }
    }

    /// Counts one record of type `record`.
    pub fn record(&mut self, record: u16) {
        *self.counts.entry(record).or_insert(0) += 1;
    }
}

/// A list to count a call's records in, `None` while the knob is off.
#[must_use]
pub fn list(kind: Kind) -> Option<List> {
    HISTOGRAM.as_ref().map(|_| List::new(kind))
}

/// Adds a finished list to the process's counts.
pub fn finish(list: Option<List>) {
    if let (Some(list), Some(histogram)) = (list, HISTOGRAM.as_ref()) {
        histogram
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(list);
    }
}

/// Writes the counts since the last commit: a command buffer is being committed.
pub fn commit() {
    if let Some(histogram) = HISTOGRAM.as_ref() {
        histogram
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .commit();
    }
}

/// A buffer range entry: `(length, id)`.
type BufferRange = (u64, u64);

/// Per-record dump state: output sink, first-use handle numbers and buffer GPU address ranges.
pub struct Dump<W: Write> {
    out: W,
    handles: BTreeMap<u64, u64>,
    buffers: BTreeMap<u64, BufferRange>,
    next_buffer: u64,
}

impl<W: Write> Dump<W> {
    /// An empty dump state writing to `out`.
    #[must_use]
    pub const fn new(out: W) -> Self {
        Self {
            out,
            handles: BTreeMap::new(),
            buffers: BTreeMap::new(),
            next_buffer: 1,
        }
    }

    /// Records a created `MTLBuffer`'s `(gpu_address, length)` range under the next buffer id.
    pub fn buffer(&mut self, gpu_address: u64, length: u64) {
        if gpu_address != 0 && length != 0 {
            let id = self.next_buffer;
            self.next_buffer += 1;
            self.buffers.insert(gpu_address, (length, id));
        }
    }

    /// Writes one line per record of the chain starting at `head`.
    ///
    /// # Safety
    ///
    /// `head` is null or the first node of a live command list whose nodes match their `type` tag
    /// and whose embedded slices (`set_bytes`, `viewports`, `scissor_rects`) are live when
    /// non-null.
    pub unsafe fn chain(&mut self, kind: &Kind, head: MemPtr) {
        let word = WORDS[kind.index()];
        let mut lines = Vec::new();
        let mut at = head;
        // SAFETY: the caller's contract.
        while let Some(cmd) = unsafe { mem_ref::<WmtcmdBase>(at) } {
            let r#type = cmd.r#type;
            let _ = write!(lines, "{word} {type}");
            match kind {
                // SAFETY: `at` is a live blit command node of `type`.
                Kind::Blit => unsafe { self.write_blit(&mut lines, r#type, at) },
                // SAFETY: `at` is a live compute command node of `type`.
                Kind::Compute => unsafe { self.write_compute(&mut lines, r#type, at) },
                // SAFETY: `at` is a live render command node of `type`.
                Kind::Render => unsafe { self.write_render(&mut lines, r#type, at) },
            }
            lines.push(b'\n');
            at = cmd.next;
        }
        if !lines.is_empty() {
            let _ = self.out.write_all(&lines);
        }
    }

    /// Appends ` <name>=#<id>` (or ` <name>=0` for the null handle) to `line`.
    fn write_handle(&mut self, line: &mut Vec<u8>, name: &str, handle: u64) {
        if handle == 0 {
            let _ = write!(line, " {name}=0");
        } else {
            let next = (self.handles.len() as u64) + 1;
            let id = *self.handles.entry(handle).or_insert(next);
            let _ = write!(line, " {name}=#{id}");
        }
    }

    /// Appends `buf#<id>+<offset>` if `word` falls inside a recorded buffer, or `0x<hex>`.
    fn write_word(&self, line: &mut Vec<u8>, word: u64) {
        if let Some((&base, &(length, id))) = self.buffers.range(..=word).next_back()
            && word - base < length
        {
            let offset = word - base;
            let _ = write!(line, "buf#{id}+{offset}");
        } else {
            let _ = write!(line, "0x{word:x}");
        }
    }

    /// Appends a `set_bytes` payload (`bytes=<length>B length=<length> index=<index>` and `words`).
    ///
    /// # Safety
    ///
    /// A non-null `bytes` with `length > 0` points to `length` live bytes.
    unsafe fn write_bytes(&self, line: &mut Vec<u8>, bytes: MemPtr, length: u64, index: u8) {
        let _ = write!(line, " bytes={length}B length={length} index={index}");
        // SAFETY: the caller's contract.
        if let Some(slice) = unsafe { mem_slice::<u8>(bytes, length) } {
            let _ = write!(line, " words=[");
            for (i, chunk) in slice.chunks(8).enumerate() {
                if i > 0 {
                    line.push(b',');
                }
                let mut raw = [0_u8; 8];
                raw[..chunk.len()].copy_from_slice(chunk);
                self.write_word(line, u64::from_le_bytes(raw));
            }
            line.push(b']');
        }
    }

    /// Appends the fields of a blit command node.
    ///
    /// # Safety
    ///
    /// `at` points to a live blit command node matching `type`.
    unsafe fn write_blit(&mut self, line: &mut Vec<u8>, r#type: u16, at: MemPtr) {
        match r#type {
            WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromBufferToBuffer>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(line, " src_offset={}", c.src_offset);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(
                    line,
                    " dst_offset={} copy_length={}",
                    c.dst_offset, c.copy_length
                );
            }
            WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromBufferToTexture>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(
                    line,
                    " src_offset={} bytes_per_row={} bytes_per_image={}",
                    c.src_offset, c.bytes_per_row, c.bytes_per_image
                );
                write_size(line, "size", &c.size);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(line, " slice={} level={}", c.slice, c.level);
                write_origin(line, "origin", &c.origin);
            }
            WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromTextureToBuffer>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(line, " slice={} level={}", c.slice, c.level);
                write_origin(line, "origin", &c.origin);
                write_size(line, "size", &c.size);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(
                    line,
                    " offset={} bytes_per_row={} bytes_per_image={}",
                    c.offset, c.bytes_per_row, c.bytes_per_image
                );
            }
            WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromTextureToTexture>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(line, " src_slice={} src_level={}", c.src_slice, c.src_level);
                write_origin(line, "src_origin", &c.src_origin);
                write_size(line, "src_size", &c.src_size);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(line, " dst_slice={} dst_level={}", c.dst_slice, c.dst_level);
                write_origin(line, "dst_origin", &c.dst_origin);
            }
            WMT_BLIT_COMMAND_GENERATE_MIPMAPS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitGenerateMipmaps>(at) };
                self.write_handle(line, "texture", c.texture);
            }
            WMT_BLIT_COMMAND_WAIT_FOR_FENCE | WMT_BLIT_COMMAND_UPDATE_FENCE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitFenceOp>(at) };
                self.write_handle(line, "fence", c.fence);
            }
            WMT_BLIT_COMMAND_FILL_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitFillbuffer>(at) };
                self.write_handle(line, "buffer", c.buffer);
                let _ = write!(
                    line,
                    " offset={} length={} value={}",
                    c.offset, c.length, c.value
                );
            }
            WMT_BLIT_COMMAND_RESOLVE_COUNTERS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitResolvecounters>(at) };
                self.write_handle(line, "sample_buffer", c.sample_buffer);
                let _ = write!(line, " start={} len={}", c.start, c.len);
                self.write_handle(line, "dst_buffer", c.dst_buffer);
                let _ = write!(line, " dst_offset={}", c.dst_offset);
            }
            WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromBufferToTextureWithblitoption>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(
                    line,
                    " src_offset={} bytes_per_row={} bytes_per_image={}",
                    c.src_offset, c.bytes_per_row, c.bytes_per_image
                );
                write_size(line, "size", &c.size);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(
                    line,
                    " slice={} level={} options={}",
                    c.slice, c.level, c.options
                );
                write_origin(line, "origin", &c.origin);
            }
            WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyFromTextureToBufferWithblitoption>(at) };
                self.write_handle(line, "src", c.src);
                let _ = write!(
                    line,
                    " slice={} level={} options={}",
                    c.slice, c.level, c.options
                );
                write_origin(line, "origin", &c.origin);
                write_size(line, "size", &c.size);
                self.write_handle(line, "dst", c.dst);
                let _ = write!(
                    line,
                    " offset={} bytes_per_row={} bytes_per_image={}",
                    c.offset, c.bytes_per_row, c.bytes_per_image
                );
            }
            WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitResetcommands>(at) };
                self.write_handle(line, "indirect_command_buffer", c.indirect_command_buffer);
                let _ = write!(line, " location={} length={}", c.location, c.length);
            }
            WMT_BLIT_COMMAND_COPY_TEXTURE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdBlitCopyTexture>(at) };
                self.write_handle(line, "src", c.src);
                self.write_handle(line, "dst", c.dst);
            }
            _ => {}
        }
    }

    /// Appends the fields of a compute command node.
    ///
    /// # Safety
    ///
    /// `at` points to a live compute command node matching `type`.
    unsafe fn write_compute(&mut self, line: &mut Vec<u8>, r#type: u16, at: MemPtr) {
        match r#type {
            WMT_COMPUTE_COMMAND_DISPATCH | WMT_COMPUTE_COMMAND_DISPATCH_THREADS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeDispatch>(at) };
                write_size(line, "size", &c.size);
            }
            WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeDispatchIndirect>(at) };
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
            }
            WMT_COMPUTE_COMMAND_SET_PSO => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeSetpso>(at) };
                self.write_handle(line, "pso", c.pso);
                write_size(line, "threadgroup_size", &c.threadgroup_size);
            }
            WMT_COMPUTE_COMMAND_SET_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeSetbuffer>(at) };
                self.write_handle(line, "buffer", c.buffer);
                let _ = write!(line, " offset={} index={}", c.offset, c.index);
            }
            WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeSetbufferoffset>(at) };
                let _ = write!(line, " offset={} index={}", c.offset, c.index);
            }
            WMT_COMPUTE_COMMAND_USE_RESOURCE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeUseresource>(at) };
                self.write_handle(line, "resource", c.resource);
                let _ = write!(line, " usage={}", c.usage);
            }
            WMT_COMPUTE_COMMAND_SET_BYTES => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeSetbytes>(at) };
                // SAFETY: the caller's contract.
                unsafe { self.write_bytes(line, c.bytes, c.length, c.index) };
            }
            WMT_COMPUTE_COMMAND_SET_TEXTURE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeSettexture>(at) };
                self.write_handle(line, "texture", c.texture);
                let _ = write!(line, " index={}", c.index);
            }
            WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE | WMT_COMPUTE_COMMAND_UPDATE_FENCE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeFenceOp>(at) };
                self.write_handle(line, "fence", c.fence);
            }
            WMT_COMPUTE_COMMAND_MEMORY_BARRIER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeMemoryBarrier>(at) };
                let _ = write!(line, " scope={}", c.scope);
            }
            WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdComputeExecutecommands>(at) };
                self.write_handle(line, "indirect_command_buffer", c.indirect_command_buffer);
                let _ = write!(line, " location={} length={}", c.location, c.length);
            }
            _ => {}
        }
    }

    /// Appends the fields of a render command node.
    ///
    /// # Safety
    ///
    /// `at` points to a live render command node matching `type`.
    unsafe fn write_render(&mut self, line: &mut Vec<u8>, r#type: u16, at: MemPtr) {
        match r#type {
            WMT_RENDER_COMMAND_USE_RESOURCE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderUseresource>(at) };
                self.write_handle(line, "resource", c.resource);
                let _ = write!(line, " usage={} stages={}", c.usage, c.stages);
            }
            WMT_RENDER_COMMAND_SET_VERTEX_BUFFER
            | WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER
            | WMT_RENDER_COMMAND_SET_MESH_BUFFER
            | WMT_RENDER_COMMAND_SET_OBJECT_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetbuffer>(at) };
                self.write_handle(line, "buffer", c.buffer);
                let _ = write!(line, " offset={} index={}", c.offset, c.index);
            }
            WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET
            | WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET
            | WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET
            | WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetbufferoffset>(at) };
                let _ = write!(line, " offset={} index={}", c.offset, c.index);
            }
            WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE | WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSettexture>(at) };
                self.write_handle(line, "texture", c.texture);
                let _ = write!(line, " index={}", c.index);
            }
            WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES | WMT_RENDER_COMMAND_SET_VERTEX_BYTES => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetbytes>(at) };
                // SAFETY: the caller's contract.
                unsafe { self.write_bytes(line, c.bytes, c.length, c.index) };
            }
            WMT_RENDER_COMMAND_SET_RASTERIZER_STATE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetrasterizerstate>(at) };
                let _ = write!(
                    line,
                    " fill_mode={} cull_mode={} depth_clip_mode={} winding={}",
                    c.fill_mode, c.cull_mode, c.depth_clip_mode, c.winding
                );
                write_f32(line, "depth_bias", c.depth_bias);
                write_f32(line, "scole_scale", c.scole_scale);
                write_f32(line, "depth_bias_clamp", c.depth_bias_clamp);
            }
            WMT_RENDER_COMMAND_SET_VIEWPORTS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetviewports>(at) };
                let count = u64::from(c.viewport_count);
                let bytes = count * (size_of::<WMTViewport>() as u64);
                let _ = write!(line, " viewports={bytes}B viewport_count={count}");
                // SAFETY: the caller's contract.
                if let Some(viewports) = unsafe { mem_slice::<WMTViewport>(c.viewports, count) } {
                    for (i, v) in viewports.iter().enumerate() {
                        write_viewport(line, &format!("viewports[{i}]"), v);
                    }
                }
            }
            WMT_RENDER_COMMAND_SET_SCISSOR_RECTS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetscissorrects>(at) };
                let count = u64::from(c.rect_count);
                let bytes = count * (size_of::<WMTScissorRect>() as u64);
                let _ = write!(line, " scissor_rects={bytes}B rect_count={count}");
                // SAFETY: the caller's contract.
                if let Some(rects) = unsafe { mem_slice::<WMTScissorRect>(c.scissor_rects, count) }
                {
                    for (i, r) in rects.iter().enumerate() {
                        write_scissor_rect(line, &format!("scissor_rects[{i}]"), r);
                    }
                }
            }
            WMT_RENDER_COMMAND_SET_PSO => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetpso>(at) };
                self.write_handle(line, "pso", c.pso);
            }
            WMT_RENDER_COMMAND_SET_DSSO => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetdsso>(at) };
                self.write_handle(line, "dsso", c.dsso);
                let _ = write!(line, " stencil_ref={}", c.stencil_ref);
            }
            WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetblendcolor>(at) };
                write_f32(line, "red", c.red);
                write_f32(line, "green", c.green);
                write_f32(line, "blue", c.blue);
                write_f32(line, "alpha", c.alpha);
                let _ = write!(line, " stencil_ref={}", c.stencil_ref);
            }
            WMT_RENDER_COMMAND_SET_VISIBILITY_MODE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetvisibilitymode>(at) };
                let _ = write!(line, " offset={} mode={}", c.offset, c.mode);
            }
            WMT_RENDER_COMMAND_DRAW => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDraw>(at) };
                let _ = write!(
                    line,
                    " primitive_type={} vertex_start={} vertex_count={} instance_count={} base_instance={}",
                    c.primitive_type,
                    c.vertex_start,
                    c.vertex_count,
                    c.instance_count,
                    c.base_instance
                );
            }
            WMT_RENDER_COMMAND_DRAW_INDEXED => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDrawIndexed>(at) };
                let _ = write!(
                    line,
                    " primitive_type={} index_type={} index_count={}",
                    c.primitive_type, c.index_type, c.index_count
                );
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(
                    line,
                    " index_buffer_offset={} instance_count={} base_vertex={} base_instance={}",
                    c.index_buffer_offset, c.instance_count, c.base_vertex, c.base_instance
                );
            }
            WMT_RENDER_COMMAND_DRAW_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDrawIndirect>(at) };
                let _ = write!(line, " primitive_type={}", c.primitive_type);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
            }
            WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDrawIndexedIndirect>(at) };
                let _ = write!(
                    line,
                    " primitive_type={} index_type={}",
                    c.primitive_type, c.index_type
                );
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(line, " index_buffer_offset={}", c.index_buffer_offset);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
            }
            WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDrawMeshthreadgroups>(at) };
                write_size(line, "threadgroup_per_grid", &c.threadgroup_per_grid);
                write_size(line, "object_threadgroup_size", &c.object_threadgroup_size);
                write_size(line, "mesh_threadgroup_size", &c.mesh_threadgroup_size);
            }
            WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDrawMeshthreadgroupsIndirect>(at) };
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
                write_size(line, "object_threadgroup_size", &c.object_threadgroup_size);
                write_size(line, "mesh_threadgroup_size", &c.mesh_threadgroup_size);
            }
            WMT_RENDER_COMMAND_MEMORY_BARRIER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderMemoryBarrier>(at) };
                let _ = write!(
                    line,
                    " scope={} stages_before={} stages_after={}",
                    c.scope, c.stages_before, c.stages_after
                );
            }
            WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtGeometryDraw>(at) };
                let _ = write!(
                    line,
                    " draw_arguments_offset={} warp_count={} instance_count={} vertex_per_warp={}",
                    c.draw_arguments_offset, c.warp_count, c.instance_count, c.vertex_per_warp
                );
            }
            WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtGeometryDrawIndexed>(at) };
                let _ = write!(line, " draw_arguments_offset={}", c.draw_arguments_offset);
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(
                    line,
                    " index_buffer_offset={} warp_count={} instance_count={} vertex_per_warp={}",
                    c.index_buffer_offset, c.warp_count, c.instance_count, c.vertex_per_warp
                );
            }
            WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtGeometryDrawIndirect>(at) };
                self.write_handle(line, "imm_draw_arguments", c.imm_draw_arguments);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
                self.write_handle(line, "dispatch_args_buffer", c.dispatch_args_buffer);
                let _ = write!(
                    line,
                    " dispatch_args_offset={} vertex_per_warp={}",
                    c.dispatch_args_offset, c.vertex_per_warp
                );
            }
            WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtGeometryDrawIndexedIndirect>(at) };
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(line, " index_buffer_offset={}", c.index_buffer_offset);
                self.write_handle(line, "imm_draw_arguments", c.imm_draw_arguments);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
                self.write_handle(line, "dispatch_args_buffer", c.dispatch_args_buffer);
                let _ = write!(
                    line,
                    " dispatch_args_offset={} vertex_per_warp={}",
                    c.dispatch_args_offset, c.vertex_per_warp
                );
            }
            WMT_RENDER_COMMAND_WAIT_FOR_FENCE | WMT_RENDER_COMMAND_UPDATE_FENCE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderFenceOp>(at) };
                self.write_handle(line, "fence", c.fence);
                let _ = write!(line, " stages={}", c.stages);
            }
            WMT_RENDER_COMMAND_SET_VIEWPORT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetviewport>(at) };
                write_viewport(line, "viewport", &c.viewport);
            }
            WMT_RENDER_COMMAND_SET_SCISSOR_RECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetscissorrect>(at) };
                write_scissor_rect(line, "scissor_rect", &c.scissor_rect);
            }
            WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtTessellationMeshDraw>(at) };
                let _ = write!(
                    line,
                    " draw_arguments_offset={} instance_count={} threads_per_patch={} patch_per_group={} patch_per_mesh_instance={}",
                    c.draw_arguments_offset,
                    c.instance_count,
                    c.threads_per_patch,
                    c.patch_per_group,
                    c.patch_per_mesh_instance
                );
            }
            WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtTessellationMeshDrawIndexed>(at) };
                let _ = write!(line, " draw_arguments_offset={}", c.draw_arguments_offset);
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(
                    line,
                    " index_buffer_offset={} instance_count={} threads_per_patch={} patch_per_group={} patch_per_mesh_instance={}",
                    c.index_buffer_offset,
                    c.instance_count,
                    c.threads_per_patch,
                    c.patch_per_group,
                    c.patch_per_mesh_instance
                );
            }
            WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtTessellationMeshDrawIndirect>(at) };
                self.write_handle(line, "imm_draw_arguments", c.imm_draw_arguments);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
                self.write_handle(line, "dispatch_args_buffer", c.dispatch_args_buffer);
                let _ = write!(
                    line,
                    " dispatch_args_offset={} threads_per_patch={} patch_per_group={}",
                    c.dispatch_args_offset, c.threads_per_patch, c.patch_per_group
                );
            }
            WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect>(at) };
                self.write_handle(line, "imm_draw_arguments", c.imm_draw_arguments);
                self.write_handle(line, "indirect_args_buffer", c.indirect_args_buffer);
                let _ = write!(line, " indirect_args_offset={}", c.indirect_args_offset);
                self.write_handle(line, "dispatch_args_buffer", c.dispatch_args_buffer);
                let _ = write!(line, " dispatch_args_offset={}", c.dispatch_args_offset);
                self.write_handle(line, "index_buffer", c.index_buffer);
                let _ = write!(
                    line,
                    " index_buffer_offset={} threads_per_patch={} patch_per_group={}",
                    c.index_buffer_offset, c.threads_per_patch, c.patch_per_group
                );
            }
            WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderDispatchThreadsPerTile>(at) };
                let _ = write!(line, " width={} height={}", c.width, c.height);
            }
            WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderExecutecommands>(at) };
                self.write_handle(line, "indirect_command_buffer", c.indirect_command_buffer);
                let _ = write!(line, " location={} length={}", c.location, c.length);
            }
            WMT_RENDER_COMMAND_SET_BLEND_FACTOR => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetblendcolor>(at) };
                write_f32(line, "red", c.red);
                write_f32(line, "green", c.green);
                write_f32(line, "blue", c.blue);
                write_f32(line, "alpha", c.alpha);
            }
            WMT_RENDER_COMMAND_SET_STENCIL_REF => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetstencilref>(at) };
                let _ = write!(line, " stencil_ref={}", c.stencil_ref);
            }
            WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetdepthstencilstate>(at) };
                self.write_handle(line, "depth_stencil_state", c.depth_stencil_state);
            }
            WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE
            | WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE => {
                // SAFETY: the caller's contract.
                let c = unsafe { node::<WmtcmdRenderSetsampler>(at) };
                self.write_handle(line, "sampler", c.sampler);
                let _ = write!(line, " index={}", c.index);
            }
            _ => {}
        }
    }
}

/// The command node at a non-null `at`, as `T`.
///
/// # Safety
///
/// `at` is non-null and points to a live, aligned `T`.
const unsafe fn node<'a, T>(at: MemPtr) -> &'a T {
    let p: *const T = core::ptr::with_exposed_provenance(u64_to_usize(at.ptr));
    // SAFETY: the caller's contract.
    unsafe { &*p }
}

/// Appends ` <name>=0x<bits>` for an `f32`.
fn write_f32(line: &mut Vec<u8>, name: &str, value: f32) {
    let _ = write!(line, " {name}=0x{:08x}", value.to_bits());
}

/// Appends ` <prefix>.{width,height,depth}` for a [`WMTSize`].
fn write_size(line: &mut Vec<u8>, prefix: &str, s: &WMTSize) {
    let _ = write!(
        line,
        " {prefix}.width={} {prefix}.height={} {prefix}.depth={}",
        s.width, s.height, s.depth
    );
}

/// Appends ` <prefix>.{x,y,z}` for a [`WMTOrigin`].
fn write_origin(line: &mut Vec<u8>, prefix: &str, o: &WMTOrigin) {
    let _ = write!(
        line,
        " {prefix}.x={} {prefix}.y={} {prefix}.z={}",
        o.x, o.y, o.z
    );
}

/// Appends ` <prefix>.{origin_x,origin_y,width,height,znear,zfar}` as `f32::to_bits` hex.
fn write_viewport(line: &mut Vec<u8>, prefix: &str, v: &WMTViewport) {
    let _ = write!(
        line,
        " {prefix}.origin_x=0x{:08x} {prefix}.origin_y=0x{:08x} {prefix}.width=0x{:08x} {prefix}.height=0x{:08x} {prefix}.znear=0x{:08x} {prefix}.zfar=0x{:08x}",
        f64_to_f32(v.origin_x).to_bits(),
        f64_to_f32(v.origin_y).to_bits(),
        f64_to_f32(v.width).to_bits(),
        f64_to_f32(v.height).to_bits(),
        f64_to_f32(v.znear).to_bits(),
        f64_to_f32(v.zfar).to_bits(),
    );
}

/// Appends ` <prefix>.{x,y,width,height}` for a [`WMTScissorRect`].
fn write_scissor_rect(line: &mut Vec<u8>, prefix: &str, r: &WMTScissorRect) {
    let _ = write!(
        line,
        " {prefix}.x={} {prefix}.y={} {prefix}.width={} {prefix}.height={}",
        r.x, r.y, r.width, r.height
    );
}

/// Records a created `MTLBuffer`'s GPU virtual address range when `DXMT_WMTCMD_DUMP` is enabled.
pub fn record_buffer(gpu_address: u64, length: u64) {
    if let Some(dump) = DUMP.as_ref() {
        dump.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .buffer(gpu_address, length);
    }
}

/// Appends one line per record in `head` when `DXMT_WMTCMD_DUMP` is enabled.
///
/// # Safety
///
/// `head` is null or the first node of a live command list whose nodes match their `type` tag.
pub unsafe fn dump(kind: &Kind, head: MemPtr) {
    if let Some(dump) = DUMP.as_ref() {
        let mut dump = dump.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: the caller's contract.
        unsafe { dump.chain(kind, head) };
    }
}

#[cfg(test)]
mod tests;
