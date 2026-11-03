//! Buffers and their allocations.

use core::{
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
use std::sync::{Arc, Mutex};

use winemetal_sys::{
    MTLBuffer_newTexture, MTLBuffer_updateContents, MTLDevice_newBuffer, MTLDevice_supportsFamily,
    MTLHeap_newBuffer, Obj,
    wire::{
        WMT_PIXEL_FORMAT_R32_SINT, WMT_PIXEL_FORMAT_R32_UINT, WMT_PIXEL_FORMAT_RG32_UINT,
        WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        WMT_RESOURCE_OPTION_CPU_CACHE_MODE_WRITE_COMBINED, WMT_TEXTURE_TYPE_TEXTURE_BUFFER,
        WMT_TEXTURE_USAGE_SHADER_ATOMIC, WMT_TEXTURE_USAGE_SHADER_READ,
        WMT_TEXTURE_USAGE_SHADER_WRITE, WMTBufferInfo, WMTGPU_FAMILY_APPLE8, WMTTextureInfo,
    },
};

use super::{
    CachedView, PAGE_SIZE, Placed, Retained, accessible, another_ref, handle, lock, mem_ptr,
    storage_options,
};

/// The smallest buffer an allocation makes, so a texture can view it.
const MIN_LENGTH: u64 = 256;
/// What `newBufferWithBytesNoCopy` wraps memory in whole pages of.
const NO_COPY_PAGE: u64 = 16384;

bitflags::bitflags! {
    /// How a buffer allocation is made and used: the C++'s `BufferAllocationFlag` bits.
    #[derive(Clone, Copy)]
    pub struct BufferAllocationFlags: u32 {
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
        /// At least one page, shared by as many suballocations as fit in it.
        const SUBALLOCATE_FROM_ONE_PAGE = 1 << 5;
        /// Memory this allocation owns: page-aligned CPU memory, or the guest's shared memory.
        const CPU_PLACED = 1 << 6;
        /// On a Metal heap.
        const ALLOCATED_ON_HEAP = 1 << 7;
    }
}

/// A buffer: its length, its device and its current allocation.
pub struct Buffer {
    length: u64,
    device: Obj,
    current: Mutex<Option<Arc<BufferAllocation>>>,
}

impl Buffer {
    /// A buffer of `length` bytes on `device`, with no allocation yet.
    #[must_use]
    pub fn new(length: u64, device: &Obj) -> Self {
        Self {
            length,
            device: another_ref(device),
            current: Mutex::new(None),
        }
    }

    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// The allocation [`Self::rename`] last made current.
    #[must_use]
    pub fn current(&self) -> Option<Arc<BufferAllocation>> {
        lock(&self.current).clone()
    }

    /// Makes `allocation` current and returns the one it replaces.
    pub fn rename(&self, allocation: Arc<BufferAllocation>) -> Option<Arc<BufferAllocation>> {
        lock(&self.current).replace(allocation)
    }

    /// A new allocation of the buffer's length in its own memory.
    #[must_use]
    pub fn allocate(&self, flags: BufferAllocationFlags) -> Arc<BufferAllocation> {
        let info = WMTBufferInfo {
            length: self.length,
            options: options(flags),
            memory: mem_ptr(0),
            gpu_address: 0,
        };
        Arc::new(BufferAllocation::new(&self.device, info, flags))
    }

    /// A new allocation of the buffer's length at `offset` in `heap`.
    #[must_use]
    pub fn allocate_on_heap(
        &self,
        flags: BufferAllocationFlags,
        heap: &Obj,
        offset: u64,
    ) -> Arc<BufferAllocation> {
        debug_assert!(
            !flags.contains(BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE),
            "heap-backed buffer doesn't support suballocation"
        );
        debug_assert!(
            !flags.contains(BufferAllocationFlags::CPU_PLACED),
            "heap doesn't support placed buffer"
        );
        let mut info = WMTBufferInfo {
            length: self.length,
            options: options(flags),
            memory: mem_ptr(0),
            gpu_address: 0,
        };
        // SAFETY: `heap` holds a live `MTLHeap`; `info` is a buffer description winemetal fills in.
        let created = unsafe { MTLHeap_newBuffer(heap.handle(), &raw mut info, offset) };
        // SAFETY: `newBuffer` returns a +1 handle or null.
        let obj = unsafe { Obj::adopt(created) };
        Arc::new(BufferAllocation::finish(
            obj,
            &info,
            flags | BufferAllocationFlags::ALLOCATED_ON_HEAP,
            Suballocation::whole(self.length),
            None,
        ))
    }

    /// A new allocation over memory the front end owns, in whole 16 KiB pages.
    ///
    /// The allocation never frees it. No heap buffer holds it, so the front end makes it resident
    /// itself.
    ///
    /// # Safety
    ///
    /// `external.ptr` is aligned to 16 KiB, and the buffer's length rounded up to 16 KiB from it is
    /// memory that stays allocated for as long as the returned allocation lives.
    #[must_use]
    pub unsafe fn allocate_over(
        &self,
        flags: BufferAllocationFlags,
        external: &External,
    ) -> Arc<BufferAllocation> {
        let info = over_info(self.length, flags, external);
        Arc::new(BufferAllocation::new(&self.device, info, flags))
    }

    /// The Metal texture viewing `allocation` as `key` says, made the first time it is asked for.
    ///
    /// The view covers every suballocation; [`BufferView::suballocation_texel`] steps between
    /// them. Its handle lives as long as `allocation`.
    pub fn view(&self, allocation: &BufferAllocation, key: BufferViewKey) -> BufferView {
        let sub = &allocation.sub;
        let texel = u64::from(key.texel_size());
        debug_assert!(texel != 0, "a buffer view's texel size");
        debug_assert!(
            texel == 0 || sub.size.is_multiple_of(texel),
            "a suballocation holds whole texels"
        );
        let suballocation_texel = sub.size.checked_div(texel).unwrap_or(0);
        let Some(buffer) = &allocation.obj else {
            return BufferView {
                texture: 0,
                gpu_resource_id: 0,
                suballocation_texel,
            };
        };
        let mut views = lock(&allocation.views);
        if let Some(v) = views.iter().find(|v| v.key == key) {
            return BufferView {
                texture: handle(v.texture.as_ref()),
                gpu_resource_id: v.gpu_resource_id,
                suballocation_texel,
            };
        }
        let total = sub.size * u64::from(sub.count);
        let format = key.format();
        let mut usage = WMT_TEXTURE_USAGE_SHADER_READ;
        let flags = allocation.flags;
        if !flags.contains(BufferAllocationFlags::GPU_READONLY)
            && flags
                .intersects(BufferAllocationFlags::GPU_MANAGED | BufferAllocationFlags::GPU_PRIVATE)
        {
            usage |= WMT_TEXTURE_USAGE_SHADER_WRITE;
            if format == WMT_PIXEL_FORMAT_R32_UINT
                || format == WMT_PIXEL_FORMAT_R32_SINT
                || (format == WMT_PIXEL_FORMAT_RG32_UINT && self.supports_apple8())
            {
                usage |= WMT_TEXTURE_USAGE_SHADER_ATOMIC;
            }
        }
        let mut info = WMTTextureInfo {
            pixel_format: format,
            width: u32::try_from(total.checked_div(texel).unwrap_or(0)).unwrap_or(u32::MAX),
            height: 1,
            depth: 1,
            array_length: 1,
            bits: (WMT_TEXTURE_TYPE_TEXTURE_BUFFER << WMTTextureInfo::TYPE_SHIFT)
                | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
                | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
                | (usage << WMTTextureInfo::USAGE_SHIFT),
            options: allocation.options,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        // SAFETY: `buffer` holds a live `MTLBuffer`; `info` is a texture description winemetal
        // fills in.
        let created = unsafe { MTLBuffer_newTexture(buffer.handle(), &raw mut info, 0, total) };
        // SAFETY: `newTexture` returns a +1 handle or null.
        let texture = unsafe { Obj::adopt(created) };
        let view = BufferView {
            texture: handle(texture.as_ref()),
            gpu_resource_id: info.gpu_resource_id,
            suballocation_texel,
        };
        views.push(CachedView {
            key,
            texture,
            gpu_resource_id: info.gpu_resource_id,
        });
        view
    }

    fn supports_apple8(&self) -> bool {
        // SAFETY: `device` holds a live `MTLDevice`.
        unsafe { MTLDevice_supportsFamily(self.device.handle(), WMTGPU_FAMILY_APPLE8) }
    }
}

/// One Metal buffer behind a [`Buffer`], cut into suballocations.
pub struct BufferAllocation {
    obj: Option<Obj>,
    options: u64,
    flags: BufferAllocationFlags,
    /// The contents' address this process can reach, or 0.
    mapped: usize,
    gpu_address: u64,
    sub: Suballocation,
    current_suballocation: AtomicU32,
    retained: Retained,
    views: Mutex<Vec<CachedView<BufferViewKey>>>,
    /// Dropped last, after the buffer and its views.
    _placed: Option<Placed>,
}

impl BufferAllocation {
    /// The `MTLBuffer`'s handle, or the null handle if Metal made none.
    #[must_use]
    pub fn buffer(&self) -> u64 {
        handle(self.obj.as_ref())
    }

    #[must_use]
    pub const fn flags(&self) -> BufferAllocationFlags {
        self.flags
    }

    #[must_use]
    pub const fn gpu_address(&self) -> u64 {
        self.gpu_address
    }

    /// The start of suballocation `suballocation` in CPU memory, or null if it has none.
    #[must_use]
    pub fn mapped_memory(&self, suballocation: u32) -> *mut u8 {
        if self.mapped == 0 {
            return ptr::null_mut();
        }
        usize::try_from(self.sub.offset(suballocation)).map_or(ptr::null_mut(), |o| {
            ptr::with_exposed_provenance_mut(self.mapped + o)
        })
    }

    #[must_use]
    pub const fn suballocation_count(&self) -> u32 {
        self.sub.count
    }

    #[must_use]
    pub const fn suballocation_size(&self) -> u64 {
        self.sub.size
    }

    #[must_use]
    pub const fn has_suballocation(&self, suballocation: u32) -> bool {
        suballocation < self.sub.count
    }

    /// Makes `suballocation` the one draws and copies use.
    pub fn use_suballocation(&self, suballocation: u32) {
        self.current_suballocation
            .store(suballocation, Ordering::Relaxed);
    }

    #[must_use]
    pub fn current_suballocation(&self) -> u32 {
        self.current_suballocation.load(Ordering::Relaxed)
    }

    /// The current suballocation's offset in the buffer.
    #[must_use]
    pub fn current_suballocation_offset(&self) -> u64 {
        self.sub.offset(self.current_suballocation())
    }

    /// The current suballocation's offset counted in `stride`s, as a texel buffer steps.
    #[must_use]
    pub fn current_suballocation_offset_in(&self, stride: u64) -> u64 {
        u64::from(self.current_suballocation()) * stride
    }

    /// Writes `data` at `offset` in suballocation `suballocation`.
    ///
    /// Through the CPU when the contents are mapped and not managed, through winemetal otherwise;
    /// a range past the buffer writes nothing.
    pub fn update_contents(&self, offset: u64, data: &[u8], suballocation: u32) {
        let start = u64::from(suballocation)
            .checked_mul(self.sub.size)
            .and_then(|s| s.checked_add(offset));
        let len = data.len() as u64;
        let fits = start
            .and_then(|s| s.checked_add(len))
            .is_some_and(|end| end <= self.sub.info_length);
        debug_assert!(fits, "update_contents past the buffer");
        let (Some(start), true) = (start, fits) else {
            return;
        };
        if self.mapped != 0 && !self.flags.contains(BufferAllocationFlags::GPU_MANAGED) {
            let Ok(start) = usize::try_from(start) else {
                return;
            };
            let dst = ptr::with_exposed_provenance_mut::<u8>(self.mapped + start);
            // SAFETY: `dst` and the `data.len()` bytes after it lie in the buffer's contents
            // (checked against its length above), which live as long as `self` and which no Rust
            // reference borrows (only raw pointers from `mapped_memory`).
            unsafe { ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len()) };
            return;
        }
        if let Some(obj) = &self.obj {
            let src = mem_ptr(data.as_ptr().expose_provenance());
            // SAFETY: `obj` holds a live `MTLBuffer`; `src` is valid for `len` bytes during the
            // call; the range lies in the buffer (checked above).
            unsafe { MTLBuffer_updateContents(obj.handle(), start, src, len) };
        }
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

    /// Makes the buffer `info` describes, in placed memory when `flags` ask for it.
    fn new(device: &Obj, mut info: WMTBufferInfo, flags: BufferAllocationFlags) -> Self {
        let sub = Suballocation::of(info.length, flags);
        info.length = sub.info_length;
        let placed = if flags.contains(BufferAllocationFlags::CPU_PLACED) {
            Placed::pages(info.length)
        } else {
            None
        };
        if let Some(p) = &placed {
            info.memory = mem_ptr(p.addr());
        }
        // SAFETY: `device` holds a live `MTLDevice`; `info` is a buffer description winemetal fills
        // in; placed memory stays allocated until the buffer is released (`_placed` drops last).
        let created = unsafe { MTLDevice_newBuffer(device.handle(), &raw mut info) };
        // SAFETY: `newBuffer` returns a +1 handle or null.
        let obj = unsafe { Obj::adopt(created) };
        Self::finish(obj, &info, flags, sub, placed)
    }

    fn finish(
        obj: Option<Obj>,
        info: &WMTBufferInfo,
        flags: BufferAllocationFlags,
        sub: Suballocation,
        placed: Option<Placed>,
    ) -> Self {
        Self {
            obj,
            options: info.options,
            flags,
            mapped: accessible(info.memory).expose_provenance(),
            gpu_address: info.gpu_address,
            sub,
            current_suballocation: AtomicU32::new(0),
            retained: Retained::default(),
            views: Mutex::new(Vec::new()),
            _placed: placed,
        }
    }
}

/// Memory a front end owns, which an allocation wraps without freeing it.
pub struct External {
    pub ptr: *mut u8,
    /// The bytes it holds from `ptr`, at least the buffer's length.
    pub len: usize,
}

/// A buffer view's description: a format and its texel size.
pub struct BufferViewDescriptor {
    /// A `WMTPixelFormat`.
    pub format: u32,
    /// The format's texel size in bytes (not 0).
    pub texel_size: u32,
}

impl BufferViewDescriptor {
    #[must_use]
    pub const fn key(&self) -> BufferViewKey {
        BufferViewKey {
            format: self.format,
            texel_size: self.texel_size,
        }
    }
}

/// A buffer view: its descriptor, as the key a front end keeps.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BufferViewKey {
    format: u32,
    texel_size: u32,
}

impl BufferViewKey {
    /// The view's `WMTPixelFormat`.
    #[must_use]
    pub const fn format(self) -> u32 {
        self.format
    }

    #[must_use]
    pub const fn texel_size(self) -> u32 {
        self.texel_size
    }
}

/// A buffer view's Metal texture, borrowed from its allocation.
pub struct BufferView {
    /// The texture's handle, or the null handle if Metal made none.
    pub texture: u64,
    pub gpu_resource_id: u64,
    /// Texels from one suballocation's start to the next's.
    pub suballocation_texel: u64,
}

/// How one Metal buffer is cut: its length and its suballocations' size and count.
pub struct Suballocation {
    /// The length the Metal buffer is made with.
    pub info_length: u64,
    pub size: u64,
    pub count: u32,
}

impl Suballocation {
    /// The C++'s cut of a buffer of `length` bytes.
    ///
    /// At least 256 bytes; one page shared by 16-byte aligned suballocations when `flags` ask for
    /// it and one fits.
    pub fn of(length: u64, flags: BufferAllocationFlags) -> Self {
        let length = length.max(MIN_LENGTH);
        if flags.contains(BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE) && length <= PAGE_SIZE {
            let size = length.next_multiple_of(16);
            return Self {
                info_length: PAGE_SIZE,
                size,
                count: u32::try_from(PAGE_SIZE / size).unwrap_or(1),
            };
        }
        Self::whole(length)
    }

    /// One suballocation of exactly `length` bytes (a heap's buffer).
    pub const fn whole(length: u64) -> Self {
        Self {
            info_length: length,
            size: length,
            count: 1,
        }
    }

    pub fn offset(&self, suballocation: u32) -> u64 {
        u64::from(suballocation) * self.size
    }
}

/// The description `allocate_over` makes its buffer from.
pub fn over_info(length: u64, flags: BufferAllocationFlags, external: &External) -> WMTBufferInfo {
    debug_assert!(
        external.len as u64 >= length,
        "external memory shorter than the buffer"
    );
    let mut options = WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED;
    if flags.contains(BufferAllocationFlags::CPU_WRITE_COMBINED) {
        options |= WMT_RESOURCE_OPTION_CPU_CACHE_MODE_WRITE_COMBINED;
    }
    WMTBufferInfo {
        length: length.next_multiple_of(NO_COPY_PAGE),
        options,
        memory: mem_ptr(external.ptr.expose_provenance()),
        gpu_address: 0,
    }
}

const fn options(flags: BufferAllocationFlags) -> u64 {
    storage_options(
        flags.contains(BufferAllocationFlags::CPU_WRITE_COMBINED),
        flags.contains(BufferAllocationFlags::CPU_INVISIBLE),
        flags.contains(BufferAllocationFlags::GPU_MANAGED),
    )
}
