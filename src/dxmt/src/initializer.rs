//! The resource initializer: the first contents of a new buffer or texture.
//!
//! A new resource is zeroed, cleared or given its initial data by commands on the initializer's
//! own Metal queue, batched until the command queue commits its next chunk: the commit calls
//! [`ResourceInitializer::flush_to_wait`], and that chunk's command buffer waits for the
//! initializer's shared event to reach the value it answers. Front-end threads record into the
//! batch at any time, under its lock.
//!
//! Behaviour is the C++'s (`dxmt_resource_initializer.*`), except that a request never fails: the
//! C++ flushes when its 1 MiB command heap is full or its zero buffer must grow, and its
//! `continue` inside `do { } while (0)` then leaves the request undone; here the batch's memory
//! grows and a request is always recorded. Written ahead for the switches of `d3d11.dll` and
//! `d3d12.dll`, which are its first callers.

use core::{alloc::Layout as MemLayout, ptr::NonNull};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use dxmt_util::float_cast::u32_to_f32;
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLBuffer_updateContents,
    MTLCommandBuffer_blitCommandEncoder, MTLCommandBuffer_commit,
    MTLCommandBuffer_encodeSignalEvent, MTLCommandBuffer_renderCommandEncoder,
    MTLCommandEncoder_endEncoding, MTLCommandQueue_commandBuffer, MTLDevice_newBuffer,
    MTLDevice_newCommandQueue, MTLDevice_newSharedEvent, MTLSharedEvent_signaledValue, Obj, Pool,
    wire::{
        MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
        WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
        WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_NOP,
        WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL, WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL,
        WMT_LOAD_ACTION_CLEAR, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
        WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE1D_ARRAY, WMT_TEXTURE_TYPE3D,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMTBufferInfo, WMTClearColor, WMTOrigin,
        WMTRenderPassInfo, WMTSize, WMTTextureInfo, WmtcmdBase, WmtcmdBlitCopyFromBufferToTexture,
        WmtcmdBlitCopyFromBufferToTextureWithblitoption, WmtcmdBlitFillbuffer,
    },
};

use crate::{
    arena::{BlockPool, Commands},
    encoder::NO_NEXT,
    format::{self, MTL_DXGI_FORMAT_EMULATED_D24},
    resource::{BufferAllocation, BufferAllocationFlags, Texture, TextureAllocation},
    ring::{LIFETIME, RingBump, RingConfig},
};

/// Bytes in a block of the batch's command memory: the C++'s heap size.
pub const CPU_HEAP: usize = 0x10_0000;
/// Bytes in a block of the upload ring: the C++'s `kResourceInitializerGpuUploadHeapSize`.
pub const UPLOAD_BLOCK: u64 = 0x200_0000;
/// The alignment of each upload in the ring.
const UPLOAD_ALIGN: u64 = 256;
/// Command buffers the initializer's queue keeps in flight.
const CHUNKS: u64 = 2;
/// A BC format's block edge, in texels.
const BC_BLOCK: u32 = 4;
const ORIGIN: WMTOrigin = WMTOrigin { x: 0, y: 0, z: 0 };

/// A subresource's initial data, as D3D11's `D3D11_SUBRESOURCE_DATA` gives it.
pub struct SubresourceData<'a> {
    pub bytes: &'a [u8],
    /// Bytes from one row (of texels, or of blocks) to the next.
    pub row_pitch: usize,
    /// Bytes from one slice of a 3D texture to the next.
    pub depth_pitch: usize,
}

/// D3D11's resource initializer.
///
/// Its own Metal queue and shared event, and the batch of commands not yet committed.
pub struct ResourceInitializer {
    device: Obj,
    queue: Option<Obj>,
    event: Option<Obj>,
    batch: Mutex<Batch>,
}

impl ResourceInitializer {
    /// An initializer on `device`.
    #[must_use]
    pub fn new(device: &Obj) -> Self {
        // SAFETY: `device` is a live device.
        let queue = unsafe { MTLDevice_newCommandQueue(device.handle(), CHUNKS) };
        // SAFETY: a new object's handle carries the reference the `Obj` takes over.
        let queue = unsafe { Obj::adopt(queue) };
        // SAFETY: `device` is a live device.
        let event = unsafe { MTLDevice_newSharedEvent(device.handle()) };
        // SAFETY: a new object's handle carries the reference the `Obj` takes over.
        let event = unsafe { Obj::adopt(event) };
        let upload = RingBump::new(
            device,
            BufferAllocationFlags::GPU_MANAGED,
            RingConfig {
                block: UPLOAD_BLOCK,
                lifetime: LIFETIME,
            },
        );
        // SAFETY: `device` holds a reference, so its handle is a live object's.
        let device = unsafe { Obj::retain(device.handle()) };
        Self {
            device: device.unwrap_or_else(|| unreachable!("an Obj's handle is never null")),
            queue,
            event,
            batch: Mutex::new(Batch {
                seq: 1,
                coherent: 0,
                records: Commands::new(Arc::new(BlockPool::new(CPU_HEAP))),
                blits: None,
                clears: Vec::new(),
                upload,
                zero: None,
                zero_len: 0,
                retained: VecDeque::new(),
            }),
        }
    }

    /// The shared event a chunk waits on for the value [`Self::flush_to_wait`] answered.
    #[must_use]
    pub fn event(&self) -> u64 {
        self.event.as_ref().map_or(0, Obj::handle)
    }

    /// Zeroes `length` bytes at `offset` of `allocation`; answers the event value that does it.
    pub fn init_buffer_zero(
        &self,
        allocation: &Arc<BufferAllocation>,
        offset: u64,
        length: u64,
    ) -> u64 {
        let mut batch = self.lock();
        batch.retain(allocation);
        batch.blit(WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: allocation.buffer(),
            offset,
            length,
            value: 0,
        });
        batch.seq
    }

    /// Zeroes subresource `level` of `slice` of `texture`, whose memory is `allocation`.
    ///
    /// A depth-stencil or render target is cleared by a render pass, any other texture copied
    /// from the zero buffer. Answers the event value that does it.
    pub fn init_texture_zero(
        &self,
        texture: &Texture,
        allocation: &Arc<TextureAllocation>,
        slice: u32,
        level: u32,
    ) -> u64 {
        let info = texture.info();
        let planes = format::depth_stencil_planar_flags(info.pixel_format);
        if planes != 0 {
            return self.clear_depth_stencil(info, allocation, slice, level, planes);
        }
        if info.usage() & WMT_TEXTURE_USAGE_RENDER_TARGET != 0 {
            return self.clear_render_target(info, allocation, slice, level);
        }
        let layout = Layout::of(info, level);
        let total = layout.bytes_per_image as u64 * u64::from(layout.depth);
        let mut batch = self.lock();
        let zero = self.zero_buffer(&mut batch, total);
        batch.retain(allocation);
        batch.blit(layout.copy(zero, 0, allocation.texture(), slice, level));
        batch.seq
    }

    /// Uploads `data` to subresource `level` of `slice` of `texture`.
    ///
    /// `allocation` is the texture's memory; answers the event value that does it.
    /// `format_flags` are the format's `MTL_DXGI_FORMAT_*` flags. Bytes the pitches name past
    /// the end of the data are left 0.
    pub fn init_texture_data(
        &self,
        texture: &Texture,
        allocation: &Arc<TextureAllocation>,
        slice: u32,
        level: u32,
        data: &SubresourceData<'_>,
        format_flags: u32,
    ) -> u64 {
        let info = texture.info();
        if info.pixel_format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
            let d24 = format_flags & u32::from(MTL_DXGI_FORMAT_EMULATED_D24) != 0;
            return self.upload_depth_stencil(info, allocation, [slice, level], data, d24);
        }
        let layout = Layout::of(info, level);
        let image = pack(data.bytes, &layout, data.row_pitch, data.depth_pitch);
        let mut batch = self.lock();
        batch.retain(allocation);
        let (buffer, offset) = batch.upload(&image);
        batch.blit(layout.copy(buffer, offset, allocation.texture(), slice, level));
        batch.seq
    }

    /// Commits the batch and answers the event value to wait for, or 0 when nothing is pending.
    pub fn flush_to_wait(&self) -> u64 {
        let mut batch = self.lock();
        if !batch.is_idle() {
            return self.flush(&mut batch);
        }
        let coherent = batch.coherent;
        batch.upload.free_blocks(coherent);
        let last = batch.seq - 1;
        if batch.coherent == last {
            return 0;
        }
        // SAFETY: the initializer's live shared event, or nil, which answers 0.
        batch.coherent = unsafe { MTLSharedEvent_signaledValue(self.event()) };
        batch.release_coherent();
        if batch.coherent == last { 0 } else { last }
    }

    fn lock(&self) -> MutexGuard<'_, Batch> {
        self.batch.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Commits the batch to the GPU, signalling its event value, which it answers.
    fn flush(&self, batch: &mut Batch) -> u64 {
        let _pool = Pool::new();
        let seq = batch.seq;
        batch.seq += 1;
        let queue = self.queue.as_ref().map_or(0, Obj::handle);
        // SAFETY: a live command queue, or nil, which answers nil.
        let cmdbuf = unsafe { MTLCommandQueue_commandBuffer(queue) };
        batch.encode(cmdbuf);
        // SAFETY: the command buffer just made (autoreleased under `_pool`) and the live event.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, self.event(), seq) };
        // SAFETY: as above.
        unsafe { MTLCommandBuffer_commit(cmdbuf) };
        batch.reset();
        // SAFETY: the initializer's live shared event, or nil, which answers 0.
        batch.coherent = unsafe { MTLSharedEvent_signaledValue(self.event()) };
        let coherent = batch.coherent;
        batch.upload.free_blocks(coherent);
        batch.release_coherent();
        seq
    }

    /// A zeroed private buffer of at least `len` bytes.
    ///
    /// A larger one replaces the last: the batch that uses the old one is committed first, its
    /// command buffer keeping the old buffer alive, and the new one is filled with zeros first.
    fn zero_buffer(&self, batch: &mut Batch, len: u64) -> u64 {
        if batch.zero_len < len {
            if batch.zero.is_some() {
                self.flush(batch);
            }
            let mut info = WMTBufferInfo {
                length: len,
                options: WMT_RESOURCE_STORAGE_MODE_PRIVATE
                    | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
                memory: mem_ptr(0),
                gpu_address: 0,
            };
            // SAFETY: a live device; `info` is a buffer description winemetal fills in.
            let buffer = unsafe { MTLDevice_newBuffer(self.device.handle(), &raw mut info) };
            // SAFETY: `newBuffer` returns a +1 handle or null.
            batch.zero = unsafe { Obj::adopt(buffer) };
            batch.zero_len = len;
            batch.blit(WmtcmdBlitFillbuffer {
                r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
                reserved: [0; 3],
                next: NO_NEXT,
                buffer,
                offset: 0,
                length: len,
                value: 0,
            });
        }
        batch.zero.as_ref().map_or(0, Obj::handle)
    }

    /// Clears the depth (bit 0 of `planes`) and stencil (bit 1) of a subresource to 0.
    fn clear_depth_stencil(
        &self,
        info: &WMTTextureInfo,
        allocation: &Arc<TextureAllocation>,
        slice: u32,
        level: u32,
        planes: u32,
    ) -> u64 {
        let mut pass = render_pass(info, level);
        let texture = allocation.texture();
        if planes & 1 != 0 {
            let depth = &mut pass.depth;
            depth.texture = texture;
            depth.clear_depth = 0.0;
            depth.load_action = WMT_LOAD_ACTION_CLEAR;
            depth.store_action = WMT_STORE_ACTION_STORE;
            depth.slice = low_u16(slice);
            depth.level = low_u16(level);
        }
        if planes & 2 != 0 {
            let stencil = &mut pass.stencil;
            stencil.texture = texture;
            stencil.clear_stencil = 0;
            stencil.load_action = WMT_LOAD_ACTION_CLEAR;
            stencil.store_action = WMT_STORE_ACTION_STORE;
            stencil.slice = low_u16(slice);
            stencil.level = low_u16(level);
        }
        let mut batch = self.lock();
        batch.retain(allocation);
        batch.clears.push(pass);
        batch.seq
    }

    /// Clears a render target's subresource to transparent black (each slice of a 3D level).
    fn clear_render_target(
        &self,
        info: &WMTTextureInfo,
        allocation: &Arc<TextureAllocation>,
        slice: u32,
        level: u32,
    ) -> u64 {
        let mut pass = render_pass(info, level);
        if info.r#type() == WMT_TEXTURE_TYPE3D {
            pass.render_target_array_length =
                info.depth.checked_shr(level).unwrap_or(0).to_le_bytes()[0];
        }
        let color = &mut pass.colors[0];
        color.texture = allocation.texture();
        color.load_action = WMT_LOAD_ACTION_CLEAR;
        color.clear_color = WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        color.store_action = WMT_STORE_ACTION_STORE;
        color.slice = low_u16(slice);
        color.level = low_u16(level);
        let mut batch = self.lock();
        batch.retain(allocation);
        batch.clears.push(pass);
        batch.seq
    }

    /// Uploads subresource `[slice, level]` of a `DEPTH32_FLOAT_STENCIL8` texture.
    ///
    /// Its depth and stencil planes are copied apart. `data` holds D24S8 (`d24`) or D32S8X24
    /// texels.
    fn upload_depth_stencil(
        &self,
        info: &WMTTextureInfo,
        allocation: &Arc<TextureAllocation>,
        [slice, level]: [u32; 2],
        data: &SubresourceData<'_>,
        d24: bool,
    ) -> u64 {
        let (width, height) = (sub(info.width, level), sub(info.height, level));
        let (depth, stencil) = split_depth_stencil(data.bytes, width, height, data.row_pitch, d24);
        let texture = allocation.texture();
        let mut batch = self.lock();
        batch.retain(allocation);
        for (bytes, bytes_per_row, options) in [
            (&depth, width * 4, WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL),
            (
                &stencil,
                width.next_multiple_of(4),
                WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL,
            ),
        ] {
            let (src, src_offset) = batch.upload(bytes);
            batch.blit(WmtcmdBlitCopyFromBufferToTextureWithblitoption {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
                reserved: [0; 3],
                next: NO_NEXT,
                src,
                src_offset,
                bytes_per_row,
                bytes_per_image: 0,
                size: WMTSize {
                    width: u64::from(width),
                    height: u64::from(height),
                    depth: 1,
                },
                dst: texture,
                slice,
                level: low_u16(level),
                options,
                origin: ORIGIN,
            });
        }
        batch.seq
    }
}

/// The commands recorded and not yet committed, and what they keep alive.
struct Batch {
    /// The event value the batch signals when committed (the C++'s `current_seq_id_`).
    seq: u64,
    /// The last event value seen signalled.
    coherent: u64,
    records: Commands,
    /// The blit records, first and last: a no-op head the others follow.
    blits: Option<(NonNull<WmtcmdBase>, NonNull<WmtcmdBase>)>,
    clears: Vec<WMTRenderPassInfo>,
    upload: RingBump,
    zero: Option<Obj>,
    zero_len: u64,
    /// The allocations the batches use, each with the event value after which it is free.
    retained: VecDeque<(u64, Arc<dyn Send + Sync>)>,
}

// SAFETY: `blits` points into `records`, the batch's own memory, which moves with it.
unsafe impl Send for Batch {}

impl Batch {
    const fn is_idle(&self) -> bool {
        self.blits.is_none() && self.clears.is_empty()
    }

    /// Keeps `allocation` alive until the GPU ran this batch.
    // ponytail: the C++ lets go at the commit; Metal keeps its objects, not placed memory.
    fn retain<T: Send + Sync + 'static>(&mut self, allocation: &Arc<T>) {
        self.retained
            .push_back((self.seq, Arc::clone(allocation) as Arc<dyn Send + Sync>));
    }

    fn release_coherent(&mut self) {
        while self
            .retained
            .front()
            .is_some_and(|(seq, _)| *seq <= self.coherent)
        {
            self.retained.pop_front();
        }
    }

    /// Appends a blit record to the batch.
    fn blit<T: Blit>(&mut self, cmd: T) {
        let (head, tail) = self.blits.unwrap_or_else(|| {
            let nop = WmtcmdBase {
                r#type: WMT_BLIT_COMMAND_NOP,
                reserved: [0; 3],
                next: NO_NEXT,
            };
            let head = place(&mut self.records, nop);
            (head, head)
        });
        let record = place(&mut self.records, cmd).cast::<WmtcmdBase>();
        // SAFETY: `tail` is the last blit record, in `records` until the next reset; a record
        // starts with `WmtcmdBase`'s fields (`Blit`).
        unsafe { (*tail.as_ptr()).next = mem_ptr(record.as_ptr().expose_provenance()) };
        self.blits = Some((head, record));
    }

    /// Copies `bytes` into the upload ring: the block's buffer and the offset in it.
    fn upload(&mut self, bytes: &[u8]) -> (u64, u64) {
        let len = bytes.len() as u64;
        let (block, offset) = self
            .upload
            .allocate(self.seq, self.coherent, len, UPLOAD_ALIGN);
        if len > 0 && block.buffer != 0 {
            let src = mem_ptr(bytes.as_ptr().expose_provenance());
            // SAFETY: a live block of the ring with `len` bytes free at `offset`; `src` is valid
            // for `len` bytes during the call.
            unsafe { MTLBuffer_updateContents(block.buffer, offset, src, len) };
        }
        (block.buffer, offset)
    }

    /// Encodes the clears, one render pass each, then the blits in one blit pass.
    fn encode(&mut self, cmdbuf: u64) {
        for pass in &mut self.clears {
            // SAFETY: a live command buffer, or nil; `pass` is a render pass description.
            let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, pass) };
            // SAFETY: the encoder just made, or nil.
            unsafe { MTLCommandEncoder_endEncoding(encoder) };
        }
        if let Some((head, _)) = self.blits {
            // SAFETY: a live command buffer, or nil.
            let encoder = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf) };
            // SAFETY: the encoder just made, or nil; `head` leads the blit records' list.
            unsafe { MTLBlitCommandEncoder_encodeCommands(encoder, head.as_ptr()) };
            // SAFETY: as above.
            unsafe { MTLCommandEncoder_endEncoding(encoder) };
        }
    }

    fn reset(&mut self) {
        self.blits = None;
        self.clears.clear();
        self.records.reset();
    }
}

/// A `wmtcmd_blit_*` record mirror of `winemetal-wire`.
///
/// # Safety
///
/// The type is `#[repr(C)]` and led by `wmtcmd_base`'s fields, so a record is linked through a
/// [`WmtcmdBase`] pointer.
unsafe trait Blit {}

// SAFETY: `wmtcmd_*` mirrors of winemetal-wire, `#[repr(C)]` and led by `wmtcmd_base`'s fields.
unsafe impl Blit for WmtcmdBlitFillbuffer {}
// SAFETY: as above.
unsafe impl Blit for WmtcmdBlitCopyFromBufferToTexture {}
// SAFETY: as above.
unsafe impl Blit for WmtcmdBlitCopyFromBufferToTextureWithblitoption {}

/// A subresource's size and its packed layout in a buffer.
struct Layout {
    width: u32,
    height: u32,
    depth: u32,
    texture_type: u32,
    bytes_per_row: usize,
    /// Rows of texels, or of blocks for a BC format.
    rows: usize,
    bytes_per_image: usize,
}

impl Layout {
    fn of(info: &WMTTextureInfo, level: u32) -> Self {
        Self::new(
            info.pixel_format,
            info.r#type(),
            [info.width, info.height, info.depth],
            level,
        )
    }

    fn new(format: u32, texture_type: u32, size: [u32; 3], level: u32) -> Self {
        let [width, height, depth] = size.map(|n| sub(n, level));
        let block = if format::is_bc(format) { BC_BLOCK } else { 1 };
        let row_blocks = width.div_ceil(block);
        let rows = height.div_ceil(block) as usize;
        let bytes_per_row = format::texel_size(format) as usize * row_blocks as usize;
        Self {
            width,
            height,
            depth,
            texture_type,
            bytes_per_row,
            rows,
            bytes_per_image: bytes_per_row * rows,
        }
    }

    const fn is_3d(&self) -> bool {
        self.texture_type == WMT_TEXTURE_TYPE3D
    }

    const fn is_1d(&self) -> bool {
        matches!(
            self.texture_type,
            WMT_TEXTURE_TYPE1D | WMT_TEXTURE_TYPE1D_ARRAY
        )
    }

    /// The record copying the packed subresource at `src_offset` of `src` to `dst`.
    fn copy(
        &self,
        src: u64,
        src_offset: u64,
        dst: u64,
        slice: u32,
        level: u32,
    ) -> WmtcmdBlitCopyFromBufferToTexture {
        WmtcmdBlitCopyFromBufferToTexture {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            src,
            src_offset,
            bytes_per_row: u32::try_from(self.bytes_per_row).unwrap_or(u32::MAX),
            bytes_per_image: if self.is_3d() {
                u32::try_from(self.bytes_per_image).unwrap_or(u32::MAX)
            } else {
                0
            },
            size: WMTSize {
                width: u64::from(self.width),
                height: u64::from(self.height),
                depth: u64::from(self.depth),
            },
            dst,
            slice,
            level,
            origin: ORIGIN,
        }
    }
}

/// `data`'s subresource packed as `layout` says.
///
/// Its rows are `row_pitch` and slices `depth_pitch` bytes apart in `data`. A 1D texture's
/// pitch is ignored, a pitch shorter than a packed row or image copies the pitch, and bytes past
/// the end of `data` stay 0.
fn pack(data: &[u8], layout: &Layout, row_pitch: usize, depth_pitch: usize) -> Vec<u8> {
    let row = layout.bytes_per_row;
    let image = layout.bytes_per_image;
    let (row_step, row_valid) = if layout.is_1d() {
        (row, row)
    } else {
        (row_pitch, row_pitch.min(row))
    };
    let (image_step, image_valid) = if layout.is_3d() {
        (depth_pitch, depth_pitch.min(image))
    } else {
        (image, image)
    };
    let mut out = vec![0; image * layout.depth as usize];
    for z in 0..layout.depth as usize {
        if row_step == row {
            copy(&mut out, z * image, data, z * image_step, image_valid);
        } else {
            for y in 0..layout.rows {
                copy(
                    &mut out,
                    z * image + y * row,
                    data,
                    z * image_step + y * row_step,
                    row_valid,
                );
            }
        }
    }
    out
}

/// A D24S8 (`d24`) or D32S8X24 subresource of `width` x `height` split into its two planes.
///
/// The depth plane has a `f32` per texel, the stencil plane a byte per texel in rows padded to 4.
fn split_depth_stencil(
    data: &[u8],
    width: u32,
    height: u32,
    row_pitch: usize,
    d24: bool,
) -> (Vec<u8>, Vec<u8>) {
    let (width, height) = (width as usize, height as usize);
    let stencil_row = width.next_multiple_of(4);
    let mut depth = vec![0; 4 * width * height];
    let mut stencil = vec![0; stencil_row * height];
    let texel = if d24 { 4 } else { 8 };
    for y in 0..height {
        for x in 0..width {
            let at = y * row_pitch + x * texel;
            let Some(src) = data.get(at..at + texel) else {
                continue;
            };
            let (d, s) = if d24 {
                let packed = u32::from_ne_bytes([src[0], src[1], src[2], src[3]]);
                let d = u32_to_f32(packed & 0xff_ffff) / u32_to_f32(0xff_ffff);
                (
                    d.to_ne_bytes(),
                    u8::try_from(packed >> 24).unwrap_or_default(),
                )
            } else {
                ([src[0], src[1], src[2], src[3]], src[4])
            };
            depth[(y * width + x) * 4..][..4].copy_from_slice(&d);
            stencil[y * stencil_row + x] = s;
        }
    }
    (depth, stencil)
}

/// Copies `len` bytes from `src` of `data` to `dst` of `out`, as far as `data` goes.
fn copy(out: &mut [u8], dst: usize, data: &[u8], src: usize, len: usize) {
    let src = data.get(src..).unwrap_or_default();
    let n = len.min(src.len());
    if let Some(out) = out.get_mut(dst..dst.saturating_add(n)) {
        out.copy_from_slice(&src[..n]);
    }
}

/// A level's edge: `n` halved `level` times, at least 1.
const fn sub(n: u32, level: u32) -> u32 {
    let n = match n.checked_shr(level) {
        Some(n) => n,
        None => 0,
    };
    if n == 0 { 1 } else { n }
}

/// `x`'s low 16 bits, as the C++'s assignment to a 16-bit field keeps them.
fn low_u16(x: u32) -> u16 {
    u16::try_from(x & 0xffff).unwrap_or_default()
}

/// `InitializeRenderPassInfo`, sized as subresource `level` of the texture `info` describes.
fn render_pass(info: &WMTTextureInfo, level: u32) -> WMTRenderPassInfo {
    // SAFETY: integers, floats and handles, for which all zeros is a value (and zeroed padding,
    // as the C++'s memset).
    let mut pass: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    for color in &mut pass.colors {
        color.store_action = WMT_STORE_ACTION_STORE;
        color.clear_color.a = 1.0;
    }
    pass.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
    pass.depth.load_action = WMT_LOAD_ACTION_CLEAR;
    pass.depth.clear_depth = 1.0;
    pass.render_target_width = sub(info.width, level);
    pass.render_target_height = sub(info.height, level);
    pass
}

/// `value` placed in `records`.
fn place<T>(records: &mut Commands, value: T) -> NonNull<T> {
    let p = records.allocate(MemLayout::new::<T>()).cast::<T>();
    // SAFETY: fresh memory of `T`'s size and alignment.
    unsafe { p.write(value) };
    p
}

/// `addr` as a pointer of `winemetal.h`'s structures.
#[cfg(target_pointer_width = "64")]
const fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr { ptr: addr as u64 }
}

/// `addr` as a pointer of `winemetal.h`'s structures: a 32-bit address and a high half of 0.
#[cfg(target_pointer_width = "32")]
fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(addr).unwrap_or(0),
        high: 0,
    }
}

#[cfg(test)]
mod tests;
