//! Textures, their allocations and their views.

use core::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use winemetal_sys::{
    MTLBuffer_newTexture, MTLDevice_newBuffer, MTLDevice_newSharedTexture, MTLDevice_newTexture,
    MTLHeap_newTexture, MTLTexture_newTextureView, Obj,
    wire::{
        WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_MANAGED,
        WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_TEXTURE_FLAG_LOSSLESS, WMT_TEXTURE_SWIZZLE_ALPHA,
        WMT_TEXTURE_SWIZZLE_BLUE, WMT_TEXTURE_SWIZZLE_GREEN, WMT_TEXTURE_SWIZZLE_RED,
        WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE1D,
        WMT_TEXTURE_TYPE1D_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTBufferInfo,
        WMTTextureInfo, WMTTextureSwizzleChannels, original_format,
    },
};

use super::{
    CachedView, Placed, Retained, accessible, another_ref, handle, lock, mem_ptr, storage_options,
};

/// A view descriptor's fields in its key: (shift, width), from the low bits up.
const FORMAT: (u32, u32) = (0, 24);
const TYPE: (u32, u32) = (24, 8);
const FIRST_MIP: (u32, u32) = (32, 4);
const MIP_COUNT: (u32, u32) = (36, 4);
const FIRST_SLICE: (u32, u32) = (40, 12);
const ARRAY_SIZE: (u32, u32) = (52, 12);

bitflags::bitflags! {
    /// How a texture allocation is made and used: the C++'s `TextureAllocationFlag` bits.
    #[derive(Clone, Copy)]
    pub struct TextureAllocationFlags: u32 {
        /// The GPU only reads it.
        const GPU_READONLY = 1;
        /// The C++'s other name for [`Self::GPU_READONLY`]'s bit.
        const NO_TRACKING = 1;
        /// Private storage: the GPU writes it.
        const GPU_PRIVATE = 1 << 1;
        /// The C++'s other name for [`Self::GPU_PRIVATE`]'s bit.
        const CPU_INVISIBLE = 1 << 1;
        /// Write-combined CPU cache mode.
        const CPU_WRITE_COMBINED = 1 << 2;
        /// A command list owns it.
        const OWNED_BY_COMMAND_LIST = 1 << 3;
        /// Managed storage.
        const GPU_MANAGED = 1 << 4;
        /// Shared with another process through a mach port.
        const SHARED = 1 << 5;
        /// Shaders only read it: one access tracker for the whole texture.
        const SHADER_READONLY = 1 << 6;
        /// On a Metal heap.
        const ALLOCATED_ON_HEAP = 1 << 7;
    }
}

/// A texture: its description, its device and its current allocation.
pub struct Texture {
    info: WMTTextureInfo,
    /// A linear texture's bytes per image, in a buffer of its own; 0 for any other.
    bytes_per_image: u32,
    bytes_per_row: u32,
    device: Obj,
    current: Mutex<Option<Arc<TextureAllocation>>>,
}

impl Texture {
    /// A texture `info` describes, on `device`, with no allocation yet.
    #[must_use]
    pub fn new(info: WMTTextureInfo, device: &Obj) -> Self {
        Self {
            info,
            bytes_per_image: 0,
            bytes_per_row: 0,
            device: another_ref(device),
            current: Mutex::new(None),
        }
    }

    /// A linear 2D texture over a buffer of `bytes_per_image`, rows `bytes_per_row` apart.
    #[must_use]
    pub fn new_linear(
        bytes_per_image: u32,
        bytes_per_row: u32,
        info: WMTTextureInfo,
        device: &Obj,
    ) -> Self {
        debug_assert!(
            info.r#type() == WMT_TEXTURE_TYPE2D,
            "a linear texture is 2D"
        );
        debug_assert!(
            info.mipmap_level_count() == 1,
            "a linear texture has one level"
        );
        debug_assert!(info.array_length == 1, "a linear texture has one slice");
        Self {
            bytes_per_image,
            bytes_per_row,
            ..Self::new(info, device)
        }
    }

    #[must_use]
    pub const fn info(&self) -> &WMTTextureInfo {
        &self.info
    }

    /// The slices a view of the whole texture has: six per cube.
    #[must_use]
    pub const fn array_length(&self) -> u32 {
        match self.info.r#type() {
            WMT_TEXTURE_TYPE_CUBE | WMT_TEXTURE_TYPE_CUBE_ARRAY => self.info.array_length * 6,
            _ => self.info.array_length,
        }
    }

    /// The view of the whole texture, in its format without a swizzle.
    #[must_use]
    pub fn full_view(&self) -> TextureViewKey {
        TextureViewDescriptor {
            format: original_format(self.info.pixel_format),
            texture_type: self.info.r#type(),
            first_mip: 0,
            mip_count: self.info.mipmap_level_count(),
            first_slice: 0,
            array_size: self.array_length(),
        }
        .key()
    }

    /// The width, height and depth of the first level `key` views.
    #[must_use]
    pub fn view_size(&self, key: TextureViewKey) -> [u32; 3] {
        let level = key.first_mip();
        [self.info.width, self.info.height, self.info.depth].map(|v| (v >> level).max(1))
    }

    /// The allocation [`Self::rename`] last made current.
    #[must_use]
    pub fn current(&self) -> Option<Arc<TextureAllocation>> {
        lock(&self.current).clone()
    }

    /// Makes `allocation` current and returns the one it replaces.
    pub fn rename(&self, allocation: Arc<TextureAllocation>) -> Option<Arc<TextureAllocation>> {
        lock(&self.current).replace(allocation)
    }

    /// A new allocation; a linear texture's in a buffer of its own.
    #[must_use]
    pub fn allocate(&self, flags: TextureAllocationFlags) -> Arc<TextureAllocation> {
        let mut info = self.info;
        info.mach_port = 0;
        info.options = options(flags);
        if self.bytes_per_image == 0 {
            let device = self.device.handle();
            let created = if flags.contains(TextureAllocationFlags::SHARED) {
                // SAFETY: `device` is held live by `self`; `info` is a texture description
                // winemetal fills in.
                unsafe { MTLDevice_newSharedTexture(device, &raw mut info) }
            } else {
                // SAFETY: as above.
                unsafe { MTLDevice_newTexture(device, &raw mut info) }
            };
            // SAFETY: `newTexture` and `newSharedTexture` return a +1 handle or null.
            let obj = unsafe { Obj::adopt(created) };
            return self.finish(obj, &info, flags, None, 0, None);
        }
        // A buffer-backed texture is never compressed.
        info.flags &= !WMT_TEXTURE_FLAG_LOSSLESS;
        // A 32-bit process places the buffer in its own memory, as the C++'s `__i386__` build.
        let placed = if cfg!(target_arch = "x86") {
            Placed::heap(u64::from(self.bytes_per_image))
        } else {
            None
        };
        let mut buffer_info = WMTBufferInfo {
            length: u64::from(self.bytes_per_image),
            options: info.options,
            memory: mem_ptr(placed.as_ref().map_or(0, Placed::addr)),
            gpu_address: 0,
        };
        // SAFETY: `device` is held live by `self`; `buffer_info` is a buffer description winemetal
        // fills in; placed memory outlives the buffer (the allocation drops it last).
        let created = unsafe { MTLDevice_newBuffer(self.device.handle(), &raw mut buffer_info) };
        // SAFETY: `newBuffer` returns a +1 handle or null.
        let buffer = unsafe { Obj::adopt(created) };
        let texture = buffer.as_ref().and_then(|b| {
            // SAFETY: `b` holds a live `MTLBuffer`; `info` is a texture description winemetal
            // fills in.
            let created = unsafe {
                MTLBuffer_newTexture(b.handle(), &raw mut info, 0, u64::from(self.bytes_per_row))
            };
            // SAFETY: `newTexture` returns a +1 handle or null.
            unsafe { Obj::adopt(created) }
        });
        let mapped = accessible(buffer_info.memory).expose_provenance();
        self.finish(texture, &info, flags, buffer, mapped, placed)
    }

    /// A new allocation at `offset` in `heap`.
    #[must_use]
    pub fn allocate_on_heap(
        &self,
        flags: TextureAllocationFlags,
        heap: &Obj,
        offset: u64,
    ) -> Arc<TextureAllocation> {
        let mut info = self.info;
        info.mach_port = 0;
        info.options = options(flags);
        // SAFETY: `heap` holds a live `MTLHeap`; `info` is a texture description winemetal fills in.
        let created = unsafe { MTLHeap_newTexture(heap.handle(), &raw mut info, offset) };
        // SAFETY: `newTexture` returns a +1 handle or null.
        let obj = unsafe { Obj::adopt(created) };
        let flags = flags | TextureAllocationFlags::ALLOCATED_ON_HEAP;
        self.finish(obj, &info, flags, None, 0, None)
    }

    /// An allocation of the texture another process shares through `mach_port`.
    ///
    /// Its flags follow the shared texture's storage and usage; `None` when Metal cannot open it.
    #[must_use]
    pub fn import(&self, mach_port: u32) -> Option<Arc<TextureAllocation>> {
        let mut info = WMTTextureInfo {
            pixel_format: 0,
            width: 0,
            height: 0,
            depth: 0,
            array_length: 0,
            bits: 0,
            options: 0,
            flags: 0,
            mach_port,
            gpu_resource_id: 0,
        };
        // SAFETY: `device` is held live by `self`; `info` names the port and winemetal fills in the
        // rest.
        let created = unsafe { MTLDevice_newSharedTexture(self.device.handle(), &raw mut info) };
        // SAFETY: `newSharedTexture` returns a +1 handle or null.
        let obj = unsafe { Obj::adopt(created) };
        debug_assert!(obj.is_some(), "failed to import shared texture");
        let obj = obj?;
        let mut flags = TextureAllocationFlags::SHARED;
        flags.set(
            TextureAllocationFlags::GPU_MANAGED,
            info.options & WMT_RESOURCE_STORAGE_MODE_MANAGED != 0,
        );
        flags.set(
            TextureAllocationFlags::GPU_PRIVATE,
            info.options & WMT_RESOURCE_STORAGE_MODE_PRIVATE != 0,
        );
        flags.set(
            TextureAllocationFlags::NO_TRACKING,
            info.options & WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED != 0,
        );
        flags.set(
            TextureAllocationFlags::SHADER_READONLY,
            info.usage() & (WMT_TEXTURE_USAGE_SHADER_WRITE | WMT_TEXTURE_USAGE_RENDER_TARGET) == 0,
        );
        Some(self.finish(Some(obj), &info, flags, None, 0, None))
    }

    /// The allocation of `texture`, sized for this texture's slices and levels.
    ///
    /// A linear texture's also holds its buffer, its contents' address and its placed memory.
    fn finish(
        &self,
        texture: Option<Obj>,
        info: &WMTTextureInfo,
        flags: TextureAllocationFlags,
        buffer: Option<Obj>,
        mapped: usize,
        placed: Option<Placed>,
    ) -> Arc<TextureAllocation> {
        Arc::new(TextureAllocation {
            obj: texture,
            buffer,
            flags,
            mapped,
            gpu_resource_id: info.gpu_resource_id,
            mach_port: info.mach_port,
            full_view: self.full_view(),
            array_length: self.array_length(),
            mip_level_count: self.info.mipmap_level_count(),
            retained: Retained::default(),
            views: Mutex::new(Vec::new()),
            _placed: placed,
        })
    }
}

/// One Metal texture behind a [`Texture`].
pub struct TextureAllocation {
    obj: Option<Obj>,
    /// A linear texture's buffer.
    buffer: Option<Obj>,
    flags: TextureAllocationFlags,
    /// A linear texture's contents in CPU memory, or 0.
    mapped: usize,
    gpu_resource_id: u64,
    mach_port: u32,
    full_view: TextureViewKey,
    array_length: u32,
    mip_level_count: u32,
    retained: Retained,
    views: Mutex<Vec<CachedView<TextureViewKey>>>,
    /// Dropped last, after the buffer and its texture.
    _placed: Option<Placed>,
}

impl TextureAllocation {
    /// The `MTLTexture`'s handle, or the null handle if Metal made none.
    #[must_use]
    pub fn texture(&self) -> u64 {
        handle(self.obj.as_ref())
    }

    /// A linear texture's `MTLBuffer` handle, or the null handle.
    #[must_use]
    pub fn buffer(&self) -> u64 {
        handle(self.buffer.as_ref())
    }

    #[must_use]
    pub const fn flags(&self) -> TextureAllocationFlags {
        self.flags
    }

    /// A linear texture's contents in CPU memory, or null.
    #[must_use]
    pub const fn mapped_memory(&self) -> *mut u8 {
        core::ptr::with_exposed_provenance_mut(self.mapped)
    }

    #[must_use]
    pub const fn gpu_resource_id(&self) -> u64 {
        self.gpu_resource_id
    }

    /// The mach port a shared texture was made or opened with, or 0.
    #[must_use]
    pub const fn mach_port(&self) -> u32 {
        self.mach_port
    }

    /// The texture's slices (six per cube) and levels, which index its access trackers.
    #[must_use]
    pub const fn array_length(&self) -> u32 {
        self.array_length
    }

    #[must_use]
    pub const fn mip_level_count(&self) -> u32 {
        self.mip_level_count
    }

    /// How many access trackers it has: one if shaders only read it, else one per subresource.
    #[must_use]
    pub const fn tracker_count(&self) -> u32 {
        if self.flags.contains(TextureAllocationFlags::SHADER_READONLY) {
            1
        } else {
            self.array_length * self.mip_level_count
        }
    }

    /// The Metal texture `key` views, made the first time it is asked for.
    ///
    /// The whole texture's view is the allocation's texture itself. Its handle lives as long as
    /// the allocation.
    pub fn view(&self, key: TextureViewKey) -> TextureView {
        if key == self.full_view {
            return TextureView {
                texture: self.texture(),
                gpu_resource_id: self.gpu_resource_id,
            };
        }
        let Some(parent) = &self.obj else {
            return TextureView {
                texture: 0,
                gpu_resource_id: 0,
            };
        };
        let mut views = lock(&self.views);
        if let Some(v) = views.iter().find(|v| v.key == key) {
            return TextureView {
                texture: handle(v.texture.as_ref()),
                gpu_resource_id: v.gpu_resource_id,
            };
        }
        let [first_mip, mip_count, first_slice, array_size] = [
            key.first_mip(),
            key.mip_count(),
            key.first_slice(),
            key.array_size(),
        ]
        .map(|v| u16::try_from(v).unwrap_or(u16::MAX));
        let identity = WMTTextureSwizzleChannels {
            r: WMT_TEXTURE_SWIZZLE_RED,
            g: WMT_TEXTURE_SWIZZLE_GREEN,
            b: WMT_TEXTURE_SWIZZLE_BLUE,
            a: WMT_TEXTURE_SWIZZLE_ALPHA,
        };
        let mut gpu_resource_id = 0;
        // SAFETY: `parent` holds a live `MTLTexture`; the out pointer is a local.
        let created = unsafe {
            MTLTexture_newTextureView(
                parent.handle(),
                key.format(),
                key.texture_type(),
                first_mip,
                mip_count,
                first_slice,
                array_size,
                identity,
                &raw mut gpu_resource_id,
            )
        };
        // SAFETY: `newTextureView` returns a +1 handle or null.
        let texture = unsafe { Obj::adopt(created) };
        let view = TextureView {
            texture: handle(texture.as_ref()),
            gpu_resource_id,
        };
        views.push(CachedView {
            key,
            texture,
            gpu_resource_id,
        });
        view
    }

    /// Records that command sequence `seq` retains it: true when `seq` had already.
    pub fn check_retained(&self, seq: u64) -> bool {
        self.retained.check(seq)
    }

    /// The last sequence [`Self::check_retained`] recorded.
    #[must_use]
    pub fn last_retained_seq(&self) -> u64 {
        self.retained.last()
    }
}

/// A texture view's description; [`Self::key`] encodes it whole.
///
/// The format holds 24 bits (a `WMTPixelFormat` with its swizzle), the type 8, the levels 4 each
/// and the slices 12 each, as the C++'s bitfields; a wider value is cut to its field.
pub struct TextureViewDescriptor {
    /// A `WMTPixelFormat`.
    pub format: u32,
    /// A `WMTTextureType`.
    pub texture_type: u32,
    pub first_mip: u32,
    pub mip_count: u32,
    pub first_slice: u32,
    pub array_size: u32,
}

impl TextureViewDescriptor {
    #[must_use]
    pub fn key(&self) -> TextureViewKey {
        TextureViewKey(
            put(self.format, FORMAT)
                | put(self.texture_type, TYPE)
                | put(self.first_mip, FIRST_MIP)
                | put(self.mip_count, MIP_COUNT)
                | put(self.first_slice, FIRST_SLICE)
                | put(self.array_size, ARRAY_SIZE),
        )
    }
}

/// A texture view: its descriptor's encoding, read without the texture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TextureViewKey(u64);

impl TextureViewKey {
    #[must_use]
    pub fn descriptor(self) -> TextureViewDescriptor {
        TextureViewDescriptor {
            format: self.format(),
            texture_type: self.texture_type(),
            first_mip: self.first_mip(),
            mip_count: self.mip_count(),
            first_slice: self.first_slice(),
            array_size: self.array_size(),
        }
    }

    /// The view's `WMTPixelFormat`.
    #[must_use]
    pub fn format(self) -> u32 {
        self.get(FORMAT)
    }

    /// The view's `WMTTextureType`.
    #[must_use]
    pub fn texture_type(self) -> u32 {
        self.get(TYPE)
    }

    #[must_use]
    pub fn first_mip(self) -> u32 {
        self.get(FIRST_MIP)
    }

    #[must_use]
    pub fn mip_count(self) -> u32 {
        self.get(MIP_COUNT)
    }

    #[must_use]
    pub fn first_slice(self) -> u32 {
        self.get(FIRST_SLICE)
    }

    #[must_use]
    pub fn array_size(self) -> u32 {
        self.get(ARRAY_SIZE)
    }

    /// The same view as an array type or not, as a shader's argument declares it.
    ///
    /// The C++'s `checkViewUseArray`: a cube becomes a cube array of 6 slices and back, any other
    /// type its array form of 1 slice and back; a 3D or texture-buffer view stays as it is.
    #[must_use]
    pub fn with_array(self, is_array: bool) -> Self {
        let (to, array_size) = match (self.texture_type(), is_array) {
            (WMT_TEXTURE_TYPE1D, true) => (WMT_TEXTURE_TYPE1D_ARRAY, 1),
            (WMT_TEXTURE_TYPE1D_ARRAY, false) => (WMT_TEXTURE_TYPE1D, 1),
            (WMT_TEXTURE_TYPE2D, true) => (WMT_TEXTURE_TYPE2D_ARRAY, 1),
            (WMT_TEXTURE_TYPE2D_ARRAY, false) => (WMT_TEXTURE_TYPE2D, 1),
            (WMT_TEXTURE_TYPE2D_MULTISAMPLE, true) => (WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, 1),
            (WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, false) => (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 1),
            (WMT_TEXTURE_TYPE_CUBE, true) => (WMT_TEXTURE_TYPE_CUBE_ARRAY, 6),
            (WMT_TEXTURE_TYPE_CUBE_ARRAY, false) => (WMT_TEXTURE_TYPE_CUBE, 6),
            _ => return self,
        };
        let mut d = self.descriptor();
        d.texture_type = to;
        d.array_size = array_size;
        d.key()
    }

    /// The same view in `format`, as the C++'s `checkViewUseFormat`.
    #[must_use]
    pub fn with_format(self, format: u32) -> Self {
        let mut d = self.descriptor();
        d.format = format;
        d.key()
    }

    fn get(self, (shift, width): (u32, u32)) -> u32 {
        u32::try_from((self.0 >> shift) & ((1 << width) - 1)).unwrap_or(0)
    }
}

/// A texture view's Metal texture, borrowed from its allocation.
pub struct TextureView {
    /// The texture's handle, or the null handle if Metal made none.
    pub texture: u64,
    pub gpu_resource_id: u64,
}

/// A ring of allocations of one texture, renamed through once per frame.
///
/// The C++'s `RenamableTexturePool`: the first call of a frame starts the ring over, each call
/// takes the next allocation, made on first use.
pub struct RenamableTexturePool {
    texture: Arc<Texture>,
    allocations: Vec<Option<Arc<TextureAllocation>>>,
    flags: TextureAllocationFlags,
    last_frame: u64,
    next: usize,
}

impl RenamableTexturePool {
    #[must_use]
    pub fn new(
        texture: Arc<Texture>,
        capacity: NonZeroUsize,
        flags: TextureAllocationFlags,
    ) -> Self {
        Self {
            texture,
            allocations: vec![None; capacity.get()],
            flags,
            last_frame: 0,
            next: 0,
        }
    }

    /// The next allocation for `frame`.
    pub fn next_allocation(&mut self, frame: u64) -> Arc<TextureAllocation> {
        if frame > self.last_frame {
            self.last_frame = frame;
            self.next = 0;
        }
        let slot = self.next % self.allocations.len();
        self.next = self.next.wrapping_add(1);
        let texture = &self.texture;
        let flags = self.flags;
        self.allocations[slot]
            .get_or_insert_with(|| texture.allocate(flags))
            .clone()
    }
}

/// `value` in its field of a view key, cut to the field's width.
fn put(value: u32, (shift, width): (u32, u32)) -> u64 {
    (u64::from(value) & ((1 << width) - 1)) << shift
}

const fn options(flags: TextureAllocationFlags) -> u64 {
    storage_options(
        flags.contains(TextureAllocationFlags::CPU_WRITE_COMBINED),
        flags.contains(TextureAllocationFlags::CPU_INVISIBLE),
        flags.contains(TextureAllocationFlags::GPU_MANAGED),
    )
}
