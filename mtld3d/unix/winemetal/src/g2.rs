//! Buffers, textures, samplers, depth-stencil states, heaps, residency sets and indirect commands.
//!
//! Slots 18 to 23, 39 to 46, 107, 132 to 143 and 145, each exported under its C name. Every send
//! goes to the object the handle names, or to nil for the null handle, and answers what the C's
//! send answered; a texture getter on a handle of another class raises, as it did.

use core::ffi::c_void;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
#[cfg(feature = "native")]
use std::sync::{Mutex, PoisonError};

use objc2::extern_methods;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSError, NSRange};
use objc2_metal::{
    MTLBuffer, MTLCommandQueue, MTLCompareFunction, MTLDepthStencilDescriptor, MTLDevice, MTLHeap,
    MTLHeapDescriptor, MTLHeapType, MTLIndirectCommandBuffer, MTLIndirectCommandBufferDescriptor,
    MTLIndirectCommandType, MTLOrigin, MTLRegion, MTLResidencySet, MTLResidencySetDescriptor, MTLResource,
    MTLResourceID, MTLResourceOptions, MTLSamplerAddressMode, MTLSamplerBorderColor, MTLSamplerDescriptor,
    MTLSamplerMinMagFilter, MTLSamplerMipFilter, MTLSamplerState, MTLSize, MTLSparsePageSize,
    MTLStencilDescriptor, MTLStencilOperation, MTLStorageMode, MTLTexture, MTLTextureDescriptor,
    MTLTextureType,
};
use winemetal_wire::{
    UnixcallGenericObjNoret, UnixcallGenericObjObjNoret, UnixcallGenericObjUint64Ret,
    UnixcallGenericObjUint64Uint64Ret, UnixcallMtlbufferNewtexture, UnixcallMtlbufferUpdatecontents,
    UnixcallMtldeviceHeapbuffersizeandalign, UnixcallMtldeviceHeaptexturesizeandalign,
    UnixcallMtldeviceNewbuffer, UnixcallMtldeviceNewdepthstencilstate, UnixcallMtldeviceNewheap,
    UnixcallMtldeviceNewicb, UnixcallMtldeviceNewresidencyset, UnixcallMtldeviceNewsamplerstate,
    UnixcallMtldeviceNewtexture, UnixcallMtlheapNewbuffer, UnixcallMtlheapNewtexture,
    UnixcallMtlresidencysetAddallocations, UnixcallMtltextureNewtextureview,
    UnixcallMtltextureReplaceregion, WMTBufferInfo, WMTDepthStencilInfo, WMTHeapInfo,
    WMTIndirectCommandBufferInfo, WMTSamplerInfo, WMTStencilInfo, WMTTextureInfo, WMT_COMPARE_FUNCTION_NEVER,
    WMT_SAMPLER_MIN_MAG_FILTER_LINEAR, WMT_SAMPLER_MIP_FILTER_LINEAR,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, mem_mut, params};
use crate::float_cast::{u64_to_usize, usize_to_u32};
use crate::format::{pixel_format, swizzle};
use crate::handle::{obj, ret_owned};
use crate::init::FORCED_MAX_ANISOTROPY;
use crate::texture::fill_texture_descriptor;

/// `MTLBuffer`'s `contents`, as the raw pointer Metal answers.
///
/// A private buffer answers null, which objc2's non-null return type would not allow.
trait RawBuffer {
    /// `-contents`.
    fn contents_ptr(&self) -> *mut c_void;
}

impl RawBuffer for ProtocolObject<dyn MTLBuffer> {
    extern_methods!(
        #[unsafe(method(contents))]
        #[unsafe(method_family = none)]
        fn contents_ptr(&self) -> *mut c_void;
    );
}

/// `MTLTexture`'s region copies, taking the guest's pointer as it is.
///
/// A null pointer reaches Metal, as it reached it from the C, where objc2's types do not take one.
trait RawTexture {
    /// `-replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:`.
    ///
    /// # Safety
    ///
    /// Metal reads the region's bytes from `bytes`.
    unsafe fn replace_region(
        &self,
        region: MTLRegion,
        level: usize,
        slice: usize,
        bytes: *const c_void,
        bytes_per_row: usize,
        bytes_per_image: usize,
    );

    /// `-getBytes:bytesPerRow:bytesPerImage:fromRegion:mipmapLevel:slice:`.
    ///
    /// # Safety
    ///
    /// Metal writes the region's bytes to `bytes`.
    unsafe fn get_bytes(
        &self,
        bytes: *mut c_void,
        bytes_per_row: usize,
        bytes_per_image: usize,
        region: MTLRegion,
        level: usize,
        slice: usize,
    );
}

impl RawTexture for ProtocolObject<dyn MTLTexture> {
    extern_methods!(
        #[unsafe(method(replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:))]
        #[unsafe(method_family = none)]
        unsafe fn replace_region(
            &self,
            region: MTLRegion,
            level: usize,
            slice: usize,
            bytes: *const c_void,
            bytes_per_row: usize,
            bytes_per_image: usize,
        );

        #[unsafe(method(getBytes:bytesPerRow:bytesPerImage:fromRegion:mipmapLevel:slice:))]
        #[unsafe(method_family = none)]
        unsafe fn get_bytes(
            &self,
            bytes: *mut c_void,
            bytes_per_row: usize,
            bytes_per_image: usize,
            region: MTLRegion,
            level: usize,
            slice: usize,
        );
    );
}

/// `MTLResidencySet`'s allocation lists, taking the guest's array as it is.
///
/// Count 0 with a null array is legal, and Metal gets the pointer the guest gave.
trait RawResidencySet {
    /// `-addAllocations:count:`.
    ///
    /// # Safety
    ///
    /// `allocations` holds `count` allocation handles.
    unsafe fn add_allocations(&self, allocations: *const *mut AnyObject, count: usize);

    /// `-removeAllocations:count:`.
    ///
    /// # Safety
    ///
    /// `allocations` holds `count` allocation handles.
    unsafe fn remove_allocations(&self, allocations: *const *mut AnyObject, count: usize);
}

impl RawResidencySet for ProtocolObject<dyn MTLResidencySet> {
    extern_methods!(
        #[unsafe(method(addAllocations:count:))]
        #[unsafe(method_family = none)]
        unsafe fn add_allocations(&self, allocations: *const *mut AnyObject, count: usize);

        #[unsafe(method(removeAllocations:count:))]
        #[unsafe(method_family = none)]
        unsafe fn remove_allocations(&self, allocations: *const *mut AnyObject, count: usize);
    );
}

/// `MTLDevice`'s residency-set constructor, with the error as the C reads it.
///
/// The error out-parameter is read whatever the answer: objc2's `Result` form would panic on a nil
/// set with no error.
trait RawDevice {
    /// `-newResidencySetWithDescriptor:error:`.
    ///
    /// # Safety
    ///
    /// `error` is null or points to a writable `NSError` pointer.
    unsafe fn new_residency_set(
        &self,
        desc: &MTLResidencySetDescriptor,
        error: *mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLResidencySet>>>;
}

impl RawDevice for ProtocolObject<dyn MTLDevice> {
    extern_methods!(
        #[unsafe(method(newResidencySetWithDescriptor:error:))]
        #[unsafe(method_family = new)]
        unsafe fn new_residency_set(
            &self,
            desc: &MTLResidencySetDescriptor,
            error: *mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLResidencySet>>>;
    );
}

/// `MTLCommandQueue`'s residency-set registration, with the set as the guest gave it.
///
/// A null set is sent as nil.
trait RawQueue {
    /// `-addResidencySet:`.
    fn add_residency_set(&self, set: Option<&ProtocolObject<dyn MTLResidencySet>>);
}

impl RawQueue for ProtocolObject<dyn MTLCommandQueue> {
    extern_methods!(
        #[unsafe(method(addResidencySet:))]
        #[unsafe(method_family = none)]
        fn add_residency_set(&self, set: Option<&ProtocolObject<dyn MTLResidencySet>>);
    );
}

/// The size of a page the native hooks allocate in: Apple silicon's 16 KiB.
#[cfg(feature = "native")]
const PAGE: u64 = 16384;

/// The host's allocator for the pages of a CPU-visible buffer: a length and the host's context.
#[cfg(feature = "native")]
type AllocHook = unsafe extern "C-unwind" fn(length: u64, ctx: *mut c_void) -> *mut c_void;

/// The host's release of pages its [`AllocHook`] gave: the pages and the host's context.
#[cfg(feature = "native")]
type FreeHook = unsafe extern "C-unwind" fn(ptr: *mut c_void, ctx: *mut c_void);

/// The native build's memory hooks, and the host's context for them.
#[cfg(feature = "native")]
struct MemoryHooks {
    alloc: Option<AllocHook>,
    free: Option<FreeHook>,
    ctx: usize,
}

/// The hooks the host set, none until it does.
///
/// A static because the resource is process-wide: the host sets one allocator, once through
/// [`set_memory_hooks`] before any device exists, and a buffer made on any device reads it, as the
/// C's statics did. The lock is never held across a hook, and atomics would do if `newBuffer`
/// ever showed up in a profile.
#[cfg(feature = "native")]
static MEMORY_HOOKS: Mutex<MemoryHooks> = Mutex::new(MemoryHooks { alloc: None, free: None, ctx: 0 });

/// The `max_anisotroy` a sampler gets: the guest's, or the forced floor for a trilinear sampler.
///
/// `DXMT_MAX_ANISOTROPY` (`forced`, 0 for none) raises the anisotropy of linear, mip-linear
/// samplers that do not compare, and never lowers one.
#[must_use]
pub const fn max_anisotropy(info: &WMTSamplerInfo, forced: u32) -> u32 {
    if forced != 0
        && info.min_filter == WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
        && info.mag_filter == WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
        && info.mip_filter == WMT_SAMPLER_MIP_FILTER_LINEAR
        && info.compare_function == WMT_COMPARE_FUNCTION_NEVER
        && info.max_anisotroy < forced
    {
        forced
    } else {
        info.max_anisotroy
    }
}

/// A heap offset, `None` for the guest's `!0`: no placement.
#[must_use]
pub const fn placement_offset(offset: u64) -> Option<usize> {
    if offset == u64::MAX { None } else { Some(u64_to_usize(offset)) }
}

/// A length rounded up to whole pages, as `newBufferWithBytesNoCopy:` takes memory.
#[cfg(feature = "native")]
#[must_use]
pub const fn page_rounded(length: u64) -> u64 {
    length.wrapping_add(PAGE - 1) / PAGE * PAGE
}

/// A resource id as the wire carries it: the `u64` inside.
///
/// objc2-metal keeps the field private.
#[must_use]
pub const fn resource_id(id: MTLResourceID) -> u64 {
    // SAFETY: `MTLResourceID` is a `#[repr(C)]` struct of one `u64`, `_impl`, so its bytes are
    // that `u64`.
    unsafe { core::mem::transmute::<MTLResourceID, u64>(id) }
}

/// A heap's enum from the guest's `u32`, widened as the C's cast did.
fn signed(value: u32) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

/// The address a guest's memory pointer holds.
const fn address(ptr: u64) -> *mut c_void {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(ptr))
}

/// The hooks as they are now, for a call to use without the lock.
#[cfg(feature = "native")]
fn hooks() -> MemoryHooks {
    let hooks = MEMORY_HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
    MemoryHooks { alloc: hooks.alloc, free: hooks.free, ctx: hooks.ctx }
}

/// The host's context as the pointer it gave.
#[cfg(feature = "native")]
const fn hook_ctx(ctx: usize) -> *mut c_void {
    core::ptr::with_exposed_provenance_mut(ctx)
}

/// What `info.memory` holds for `buffer`: its contents, null for a private buffer or none.
fn buffer_memory(buffer: Option<&ProtocolObject<dyn MTLBuffer>>) -> u64 {
    buffer.map_or(0, |buffer| {
        if buffer.storageMode() == MTLStorageMode::Private {
            0
        } else {
            buffer.contents_ptr().expose_provenance() as u64
        }
    })
}

/// A sampler's descriptor from the guest's, for `max_anisotropy` of the forced floor.
fn sampler_descriptor(info: &WMTSamplerInfo, forced: u32) -> Retained<MTLSamplerDescriptor> {
    let desc = MTLSamplerDescriptor::new();
    desc.setBorderColor(MTLSamplerBorderColor(usize::from(info.border_color)));
    desc.setRAddressMode(MTLSamplerAddressMode(usize::from(info.r_address_mode)));
    desc.setSAddressMode(MTLSamplerAddressMode(usize::from(info.s_address_mode)));
    desc.setTAddressMode(MTLSamplerAddressMode(usize::from(info.t_address_mode)));
    desc.setMagFilter(MTLSamplerMinMagFilter(usize::from(info.mag_filter)));
    desc.setMinFilter(MTLSamplerMinMagFilter(usize::from(info.min_filter)));
    desc.setMipFilter(MTLSamplerMipFilter(usize::from(info.mip_filter)));
    desc.setCompareFunction(MTLCompareFunction(usize::from(info.compare_function)));
    desc.setLodMaxClamp(info.lod_max_clamp);
    desc.setLodMinClamp(info.lod_min_clamp);
    desc.setMaxAnisotropy(max_anisotropy(info, forced) as usize);
    desc.setLodAverage(info.lod_average != 0);
    desc.setNormalizedCoordinates(info.normalized_coords != 0);
    desc.setSupportArgumentBuffers(info.support_argument_buffers != 0);
    desc
}

/// One face of a depth-stencil descriptor from the guest's.
fn set_stencil(stencil: &MTLStencilDescriptor, info: WMTStencilInfo) {
    stencil.setDepthStencilPassOperation(MTLStencilOperation(usize::from(info.depth_stencil_pass_op)));
    stencil.setDepthFailureOperation(MTLStencilOperation(usize::from(info.depth_fail_op)));
    stencil.setStencilFailureOperation(MTLStencilOperation(usize::from(info.stencil_fail_op)));
    stencil.setStencilCompareFunction(MTLCompareFunction(usize::from(info.stencil_compare_function)));
    stencil.setWriteMask(u32::from(info.write_mask));
    stencil.setReadMask(u32::from(info.read_mask));
}

/// The region of a texture copy.
const fn region(origin: &winemetal_wire::WMTOrigin, size: &winemetal_wire::WMTSize) -> MTLRegion {
    MTLRegion {
        origin: MTLOrigin { x: u64_to_usize(origin.x), y: u64_to_usize(origin.y), z: u64_to_usize(origin.z) },
        size: MTLSize {
            width: u64_to_usize(size.width),
            height: u64_to_usize(size.height),
            depth: u64_to_usize(size.depth),
        },
    }
}

/// A buffer over memory the native hooks gave, nil if they gave none.
///
/// Whole pages, as `newBufferWithBytesNoCopy:` takes them. The hook allocates even for a nil
/// device, as the C's did, and frees what no buffer took.
#[cfg(feature = "native")]
fn hooked_buffer(
    device: Option<&ProtocolObject<dyn MTLDevice>>,
    info: &mut WMTBufferInfo,
    hooks: &MemoryHooks,
) -> Option<Retained<ProtocolObject<dyn MTLBuffer>>> {
    let length = page_rounded(info.length);
    // SAFETY: the host's hook, called as it asked: a length and its context.
    let memory = unsafe { (hooks.alloc?)(length, hook_ctx(hooks.ctx)) };
    let memory_ptr = NonNull::new(memory)?;
    let (free, ctx) = (hooks.free, hooks.ctx);
    let release = block2::RcBlock::new(move |ptr: NonNull<c_void>, _length: usize| {
        if let Some(free) = free {
            // SAFETY: the host's hook, given back the memory it allocated.
            unsafe { free(ptr.as_ptr(), hook_ctx(ctx)) };
        }
    });
    let options = MTLResourceOptions(u64_to_usize(info.options));
    // SAFETY: `memory_ptr` is `length` bytes of the host's pages, which the release block frees
    // once Metal is done with the buffer.
    let buffer = device.and_then(|device| unsafe {
        device.newBufferWithBytesNoCopy_length_options_deallocator(
            memory_ptr,
            u64_to_usize(length),
            options,
            Some(&release),
        )
    });
    if buffer.is_some() {
        info.memory.ptr = memory.expose_provenance() as u64;
    } else if let Some(free) = hooks.free {
        // SAFETY: the host's hook, given back the memory it allocated.
        unsafe { free(memory, hook_ctx(hooks.ctx)) };
    }
    buffer
}

/// `WMTSetMemoryHooks`: the host's allocator for CPU-visible buffers, and its context.
///
/// With an allocator set, a buffer of no memory of its own that is not private or memoryless
/// lives in pages the host allocated, which it is asked to free when the buffer is released. None
/// takes the hooks away.
#[cfg(feature = "native")]
#[unsafe(export_name = "WMTSetMemoryHooks")]
pub extern "C-unwind" fn set_memory_hooks(alloc: Option<AllocHook>, free: Option<FreeHook>, ctx: *mut c_void) {
    let _guard = AbortOnPanic;
    *MEMORY_HOOKS.lock().unwrap_or_else(PoisonError::into_inner) =
        MemoryHooks { alloc, free, ctx: ctx.expose_provenance() };
}

/// `WMTMemoryHooked`: whether the host set an allocator, as a C `int`.
#[cfg(feature = "native")]
#[unsafe(export_name = "WMTMemoryHooked")]
pub extern "C-unwind" fn memory_hooked() -> i32 {
    let _guard = AbortOnPanic;
    i32::from(MEMORY_HOOKS.lock().unwrap_or_else(PoisonError::into_inner).alloc.is_some())
}

/// Slot 18, `MTLDevice_newBuffer`: a buffer over the guest's memory, the host's pages, or Metal's.
///
/// # Safety
///
/// `args` is slot 18's params block ([`UnixcallMtldeviceNewbuffer`]), its handle a live device.
#[unsafe(export_name = "_MTLDevice_newBuffer")]
pub unsafe extern "C-unwind" fn new_buffer(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewbuffer>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTBufferInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTBufferInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    #[cfg(feature = "native")]
    {
        let hooks = hooks();
        // The host's pages for a CPU-visible buffer of no memory of its own.
        let storage = info.options & 0xf0;
        if info.memory.ptr == 0
            && hooks.alloc.is_some()
            && storage != winemetal_wire::WMT_RESOURCE_STORAGE_MODE_PRIVATE
            && storage != winemetal_wire::WMT_RESOURCE_STORAGE_MODE_MEMORYLESS
        {
            let buffer = hooked_buffer(device, info, &hooks);
            info.gpu_address = buffer.as_deref().map_or(0, MTLBuffer::gpuAddress);
            p.ret = ret_owned(buffer);
            return STATUS_SUCCESS;
        }
    }
    let options = MTLResourceOptions(u64_to_usize(info.options));
    let length = u64_to_usize(info.length);
    let buffer = if let Some(memory) = NonNull::new(address(info.memory.ptr)) {
        // SAFETY: the guest's memory, which outlives the buffer; Metal has no deallocator to run.
        device.and_then(|device| unsafe {
            device.newBufferWithBytesNoCopy_length_options_deallocator(memory, length, options, None)
        })
    } else {
        let buffer = device.and_then(|device| device.newBufferWithLength_options(length, options));
        info.memory.ptr = buffer_memory(buffer.as_deref());
        buffer
    };
    info.gpu_address = buffer.as_deref().map_or(0, MTLBuffer::gpuAddress);
    p.ret = ret_owned(buffer);
    STATUS_SUCCESS
}

/// Slot 19, `MTLDevice_newSamplerState`: a sampler from the guest's description.
///
/// # Safety
///
/// `args` is slot 19's params block ([`UnixcallMtldeviceNewsamplerstate`]), its handle a live
/// device.
#[unsafe(export_name = "_MTLDevice_newSamplerState")]
pub unsafe extern "C-unwind" fn new_sampler_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewsamplerstate>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTSamplerInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTSamplerInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = sampler_descriptor(info, FORCED_MAX_ANISOTROPY.load(Ordering::Relaxed));
    let sampler = device.and_then(|device| device.newSamplerStateWithDescriptor(&desc));
    info.gpu_resource_id = if info.support_argument_buffers == 0 {
        0
    } else {
        sampler.as_deref().map_or(0, |sampler| resource_id(sampler.gpuResourceID()))
    };
    p.ret = ret_owned(sampler);
    STATUS_SUCCESS
}

/// Slot 20, `MTLDevice_newDepthStencilState`: a depth-stencil state, a stencil face when enabled.
///
/// # Safety
///
/// `args` is slot 20's params block ([`UnixcallMtldeviceNewdepthstencilstate`]), its handle a live
/// device.
#[unsafe(export_name = "_MTLDevice_newDepthStencilState")]
pub unsafe extern "C-unwind" fn new_depth_stencil_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewdepthstencilstate>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTDepthStencilInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTDepthStencilInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLDepthStencilDescriptor::new();
    desc.setDepthCompareFunction(MTLCompareFunction(usize::from(info.depth_compare_function)));
    desc.setDepthWriteEnabled(info.depth_write_enabled != 0);
    if info.front_stencil.enabled != 0 {
        set_stencil(&desc.frontFaceStencil(), info.front_stencil);
    }
    if info.back_stencil.enabled != 0 {
        set_stencil(&desc.backFaceStencil(), info.back_stencil);
    }
    p.ret = ret_owned(device.and_then(|device| device.newDepthStencilStateWithDescriptor(&desc)));
    STATUS_SUCCESS
}

/// Slot 21, `MTLDevice_newTexture`: a texture from the guest's description.
///
/// # Safety
///
/// `args` is slot 21's params block ([`UnixcallMtldeviceNewtexture`]), its handle a live device.
#[unsafe(export_name = "_MTLDevice_newTexture")]
pub unsafe extern "C-unwind" fn new_texture(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewtexture>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTTextureInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTTextureInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLTextureDescriptor::new();
    fill_texture_descriptor(&desc, info);
    let texture = device.and_then(|device| device.newTextureWithDescriptor(&desc));
    info.gpu_resource_id = texture.as_deref().map_or(0, |texture| resource_id(texture.gpuResourceID()));
    info.mach_port = 0;
    p.ret = ret_owned(texture);
    STATUS_SUCCESS
}

/// Slot 22, `MTLBuffer_newTexture`: a texture over a buffer's bytes.
///
/// # Safety
///
/// `args` is slot 22's params block ([`UnixcallMtlbufferNewtexture`]), its handle a live buffer.
#[unsafe(export_name = "_MTLBuffer_newTexture")]
pub unsafe extern "C-unwind" fn buffer_new_texture(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlbufferNewtexture>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTTextureInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTTextureInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLBuffer`, or the null handle.
    let buffer = unsafe { obj::<ProtocolObject<dyn MTLBuffer>>(p.buffer) };
    let desc = MTLTextureDescriptor::new();
    fill_texture_descriptor(&desc, info);
    let texture = buffer.and_then(|buffer| {
        buffer.newTextureWithDescriptor_offset_bytesPerRow(
            &desc,
            u64_to_usize(p.offset),
            u64_to_usize(p.bytes_per_row),
        )
    });
    info.gpu_resource_id = texture.as_deref().map_or(0, |texture| resource_id(texture.gpuResourceID()));
    info.mach_port = 0;
    p.ret = ret_owned(texture);
    STATUS_SUCCESS
}

/// Slot 23, `MTLTexture_newTextureView`: a view of a texture with a decoded swizzle.
///
/// # Safety
///
/// `args` is slot 23's params block ([`UnixcallMtltextureNewtextureview`]), its handle a live
/// texture.
#[unsafe(export_name = "_MTLTexture_newTextureView")]
pub unsafe extern "C-unwind" fn texture_new_texture_view(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtltextureNewtextureview>(args) };
    // SAFETY: the caller's contract: a live `MTLTexture`, or the null handle.
    let texture = unsafe { obj::<ProtocolObject<dyn MTLTexture>>(p.texture) };
    let view = texture.and_then(|texture| {
        // SAFETY: Metal checks the ranges against the texture and raises, as it did for the C.
        unsafe {
            texture.newTextureViewWithPixelFormat_textureType_levels_slices_swizzle(
                pixel_format(p.format),
                MTLTextureType(p.texture_type as usize),
                NSRange::new(usize::from(p.level_start), usize::from(p.level_count)),
                NSRange::new(usize::from(p.slice_start), usize::from(p.slice_count)),
                swizzle(p.format, p.swizzle),
            )
        }
    });
    p.gpu_resource_id = view.as_deref().map_or(0, |view| resource_id(view.gpuResourceID()));
    p.ret = ret_owned(view);
    STATUS_SUCCESS
}

/// A texture getter's answer, 0 for the null handle: `value` of the texture slot's handle.
///
/// # Safety
///
/// `args` is a `UnixcallGenericObjUint64Ret` whose handle is a live texture or null.
unsafe fn texture_value(args: *mut c_void, value: impl FnOnce(&ProtocolObject<dyn MTLTexture>) -> usize) {
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: the caller's contract: a live `MTLTexture`, or the null handle.
    let texture = unsafe { obj::<ProtocolObject<dyn MTLTexture>>(p.handle) };
    p.ret = texture.map_or(0, |texture| value(texture) as u64);
}

/// Slot 39, `MTLTexture_pixelFormat`: a texture's pixel format.
///
/// # Safety
///
/// `args` is slot 39's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_pixelFormat")]
pub unsafe extern "C-unwind" fn texture_pixel_format(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, |texture| texture.pixelFormat().0) };
    STATUS_SUCCESS
}

/// Slot 40, `MTLTexture_width`: a texture's width.
///
/// # Safety
///
/// `args` is slot 40's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_width")]
pub unsafe extern "C-unwind" fn texture_width(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, MTLTexture::width) };
    STATUS_SUCCESS
}

/// Slot 41, `MTLTexture_height`: a texture's height.
///
/// # Safety
///
/// `args` is slot 41's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_height")]
pub unsafe extern "C-unwind" fn texture_height(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, MTLTexture::height) };
    STATUS_SUCCESS
}

/// Slot 42, `MTLTexture_depth`: a texture's depth.
///
/// # Safety
///
/// `args` is slot 42's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_depth")]
pub unsafe extern "C-unwind" fn texture_depth(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, MTLTexture::depth) };
    STATUS_SUCCESS
}

/// Slot 43, `MTLTexture_arrayLength`: a texture's array length.
///
/// # Safety
///
/// `args` is slot 43's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_arrayLength")]
pub unsafe extern "C-unwind" fn texture_array_length(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, MTLTexture::arrayLength) };
    STATUS_SUCCESS
}

/// Slot 44, `MTLTexture_mipmapLevelCount`: a texture's mip level count.
///
/// # Safety
///
/// `args` is slot 44's params block ([`UnixcallGenericObjUint64Ret`]), its handle a live texture.
#[unsafe(export_name = "_MTLTexture_mipmapLevelCount")]
pub unsafe extern "C-unwind" fn texture_mipmap_level_count(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    unsafe { texture_value(args, MTLTexture::mipmapLevelCount) };
    STATUS_SUCCESS
}

/// Slot 45, `MTLTexture_replaceRegion`: the guest's bytes into a region of a texture.
///
/// # Safety
///
/// `args` is slot 45's params block ([`UnixcallMtltextureReplaceregion`]), its handle a live
/// texture and its data the region's bytes.
#[unsafe(export_name = "_MTLTexture_replaceRegion")]
pub unsafe extern "C-unwind" fn texture_replace_region(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtltextureReplaceregion>(args) };
    // SAFETY: the caller's contract: a live `MTLTexture`, or the null handle.
    let texture = unsafe { obj::<ProtocolObject<dyn MTLTexture>>(p.texture) };
    if let Some(texture) = texture {
        // SAFETY: the caller's contract: `data` holds the region's bytes, which Metal reads.
        unsafe {
            texture.replace_region(
                region(&p.origin, &p.size),
                u64_to_usize(p.level),
                u64_to_usize(p.slice),
                address(p.data.ptr),
                u64_to_usize(p.bytes_per_row),
                u64_to_usize(p.bytes_per_image),
            );
        }
    }
    STATUS_SUCCESS
}

/// Slot 46, `MTLBuffer_didModifyRange`: a range of a managed buffer the CPU wrote.
///
/// The range is `(arg, ret)`: the PE thunk passes the length in `ret`, which stays as it is.
///
/// # Safety
///
/// `args` is slot 46's params block ([`UnixcallGenericObjUint64Uint64Ret`]), its handle a live
/// buffer.
#[unsafe(export_name = "_MTLBuffer_didModifyRange")]
pub unsafe extern "C-unwind" fn buffer_did_modify_range(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: the caller's contract: a live `MTLBuffer`, or the null handle.
    let buffer = unsafe { obj::<ProtocolObject<dyn MTLBuffer>>(p.handle) };
    if let Some(buffer) = buffer {
        buffer.didModifyRange(NSRange::new(u64_to_usize(p.arg), u64_to_usize(p.ret)));
    }
    STATUS_SUCCESS
}

/// Slot 107, `MTLBuffer_updateContents`: the guest's bytes into a buffer's contents.
///
/// A managed buffer is told the range. A null buffer, or null contents or data for a non-empty
/// copy, writes nothing, where the C faulted.
///
/// # Safety
///
/// `args` is slot 107's params block ([`UnixcallMtlbufferUpdatecontents`]), its handle a live
/// buffer, its data `length` bytes and the offset and length inside the buffer.
#[unsafe(export_name = "_MTLBuffer_updateContents")]
pub unsafe extern "C-unwind" fn buffer_update_contents(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlbufferUpdatecontents>(args) };
    // SAFETY: the caller's contract: a live `MTLBuffer`, or the null handle.
    let Some(buffer) = (unsafe { obj::<ProtocolObject<dyn MTLBuffer>>(p.buffer) }) else {
        return STATUS_SUCCESS;
    };
    let offset = u64_to_usize(p.offset);
    let length = u64_to_usize(p.length);
    if length != 0 {
        let source = address(p.data.ptr).cast::<u8>();
        let contents = buffer.contents_ptr().cast::<u8>();
        if source.is_null() || contents.is_null() {
            return STATUS_SUCCESS;
        }
        // SAFETY: the caller's contract: `length` bytes at the offset lie in the buffer, and
        // `data` holds `length` bytes.
        unsafe { core::ptr::copy(source, contents.wrapping_add(offset), length) };
    }
    if buffer.storageMode() == MTLStorageMode::Managed {
        buffer.didModifyRange(NSRange::new(offset, length));
    }
    STATUS_SUCCESS
}

/// Slot 132, `MTLDevice_newResidencySet`: a residency set, and the error Metal left.
///
/// # Safety
///
/// `args` is slot 132's params block ([`UnixcallMtldeviceNewresidencyset`]), its handle a live
/// device.
#[unsafe(export_name = "_MTLDevice_newResidencySet")]
pub unsafe extern "C-unwind" fn new_residency_set(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewresidencyset>(args) };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLResidencySetDescriptor::new();
    // SAFETY: Metal checks the capacity when the set is made, as it did for the C.
    unsafe { desc.setInitialCapacity(u64_to_usize(p.init_capacity)) };
    let mut error: *mut NSError = core::ptr::null_mut();
    // SAFETY: `error` is a writable `NSError` pointer.
    let set = device.and_then(|device| unsafe { device.new_residency_set(&desc, &raw mut error) });
    // SAFETY: `error` is null or the error Metal left, autoreleased; the guest gets a reference.
    p.ret_error = ret_owned(unsafe { Retained::retain(error) });
    p.ret_set = ret_owned(set);
    STATUS_SUCCESS
}

/// Slot 133, `MTLResidencySet_addAllocations`: allocations into a residency set.
///
/// # Safety
///
/// `args` is slot 133's params block ([`UnixcallMtlresidencysetAddallocations`]), its handle a
/// live set and its array `count` allocation handles.
#[unsafe(export_name = "_MTLResidencySet_addAllocations")]
pub unsafe extern "C-unwind" fn residency_set_add_allocations(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlresidencysetAddallocations>(args) };
    // SAFETY: the caller's contract: a live `MTLResidencySet`, or the null handle.
    let set = unsafe { obj::<ProtocolObject<dyn MTLResidencySet>>(p.set) };
    if let Some(set) = set {
        // SAFETY: the caller's contract: the array holds `count` handles; Metal gets it as it is.
        unsafe { set.add_allocations(address(p.allocations.ptr).cast(), u64_to_usize(p.count)) };
    }
    STATUS_SUCCESS
}

/// Slot 134, `MTLResidencySet_removeAllocations`: allocations out of a residency set.
///
/// # Safety
///
/// `args` is slot 134's params block ([`UnixcallMtlresidencysetAddallocations`]), its handle a
/// live set and its array `count` allocation handles.
#[unsafe(export_name = "_MTLResidencySet_removeAllocations")]
pub unsafe extern "C-unwind" fn residency_set_remove_allocations(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlresidencysetAddallocations>(args) };
    // SAFETY: the caller's contract: a live `MTLResidencySet`, or the null handle.
    let set = unsafe { obj::<ProtocolObject<dyn MTLResidencySet>>(p.set) };
    if let Some(set) = set {
        // SAFETY: the caller's contract: the array holds `count` handles; Metal gets it as it is.
        unsafe { set.remove_allocations(address(p.allocations.ptr).cast(), u64_to_usize(p.count)) };
    }
    STATUS_SUCCESS
}

/// Slot 135, `MTLResidencySet_removeAllAllocations`: every allocation out of a residency set.
///
/// # Safety
///
/// `args` is slot 135's params block ([`UnixcallGenericObjNoret`]), its handle a live set.
#[unsafe(export_name = "_MTLResidencySet_removeAllAllocations")]
pub unsafe extern "C-unwind" fn residency_set_remove_all_allocations(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: the caller's contract: a live `MTLResidencySet`, or the null handle.
    let set = unsafe { obj::<ProtocolObject<dyn MTLResidencySet>>(p.handle) };
    if let Some(set) = set {
        set.removeAllAllocations();
    }
    STATUS_SUCCESS
}

/// Slot 136, `MTLResidencySet_commit`: a residency set's pending changes.
///
/// # Safety
///
/// `args` is slot 136's params block ([`UnixcallGenericObjNoret`]), its handle a live set.
#[unsafe(export_name = "_MTLResidencySet_commit")]
pub unsafe extern "C-unwind" fn residency_set_commit(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: the caller's contract: a live `MTLResidencySet`, or the null handle.
    let set = unsafe { obj::<ProtocolObject<dyn MTLResidencySet>>(p.handle) };
    if let Some(set) = set {
        set.commit();
    }
    STATUS_SUCCESS
}

/// Slot 137, `MTLCommandQueue_addResidencySet`: a residency set on a queue.
///
/// # Safety
///
/// `args` is slot 137's params block ([`UnixcallGenericObjObjNoret`]), its handles a live queue
/// and a live set.
#[unsafe(export_name = "_MTLCommandQueue_addResidencySet")]
pub unsafe extern "C-unwind" fn queue_add_residency_set(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjNoret>(args) };
    // SAFETY: the caller's contract: a live `MTLCommandQueue`, or the null handle.
    let queue = unsafe { obj::<ProtocolObject<dyn MTLCommandQueue>>(p.handle) };
    // SAFETY: the caller's contract: a live `MTLResidencySet`, or the null handle.
    let set = unsafe { obj::<ProtocolObject<dyn MTLResidencySet>>(p.arg) };
    if let Some(queue) = queue {
        queue.add_residency_set(set);
    }
    STATUS_SUCCESS
}

/// Slot 138, `MTLDevice_newHeap`: a heap from the guest's description.
///
/// # Safety
///
/// `args` is slot 138's params block ([`UnixcallMtldeviceNewheap`]), its handle a live device.
#[unsafe(export_name = "_MTLDevice_newHeap")]
pub unsafe extern "C-unwind" fn new_heap(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewheap>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTHeapInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTHeapInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLHeapDescriptor::new();
    desc.setResourceOptions(MTLResourceOptions(u64_to_usize(info.options)));
    desc.setSparsePageSize(MTLSparsePageSize(signed(info.sparse_page_size)));
    desc.setType(MTLHeapType(signed(info.r#type)));
    desc.setSize(u64_to_usize(info.size));
    p.ret = ret_owned(device.and_then(|device| device.newHeapWithDescriptor(&desc)));
    STATUS_SUCCESS
}

/// Slot 139, `MTLDevice_heapBufferSizeAndAlign`: the heap size and alignment of a buffer.
///
/// # Safety
///
/// `args` is slot 139's params block ([`UnixcallMtldeviceHeapbuffersizeandalign`]), its handle a
/// live device.
#[unsafe(export_name = "_MTLDevice_heapBufferSizeAndAlign")]
pub unsafe extern "C-unwind" fn heap_buffer_size_and_align(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceHeapbuffersizeandalign>(args) };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let options = MTLResourceOptions(u64_to_usize(p.options));
    let (size, align) = device.map_or((0, 0), |device| {
        let size_and_align = device.heapBufferSizeAndAlignWithLength_options(u64_to_usize(p.length), options);
        (size_and_align.size, size_and_align.align)
    });
    p.ret_size = usize_to_u32(size);
    p.ret_align = usize_to_u32(align);
    STATUS_SUCCESS
}

/// Slot 140, `MTLDevice_heapTextureSizeAndAlign`: the heap size and alignment of a texture.
///
/// # Safety
///
/// `args` is slot 140's params block ([`UnixcallMtldeviceHeaptexturesizeandalign`]), its handle a
/// live device.
#[unsafe(export_name = "_MTLDevice_heapTextureSizeAndAlign")]
pub unsafe extern "C-unwind" fn heap_texture_size_and_align(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceHeaptexturesizeandalign>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTTextureInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTTextureInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLTextureDescriptor::new();
    fill_texture_descriptor(&desc, info);
    let (size, align) = device.map_or((0, 0), |device| {
        let size_and_align = device.heapTextureSizeAndAlignWithDescriptor(&desc);
        (size_and_align.size, size_and_align.align)
    });
    p.ret_size = usize_to_u32(size);
    p.ret_align = usize_to_u32(align);
    STATUS_SUCCESS
}

/// Slot 141, `MTLHeap_newBuffer`: a buffer in a heap, at an offset unless `!0`.
///
/// # Safety
///
/// `args` is slot 141's params block ([`UnixcallMtlheapNewbuffer`]), its handle a live heap.
#[unsafe(export_name = "_MTLHeap_newBuffer")]
pub unsafe extern "C-unwind" fn heap_new_buffer(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlheapNewbuffer>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTBufferInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTBufferInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLHeap`, or the null handle.
    let heap = unsafe { obj::<ProtocolObject<dyn MTLHeap>>(p.heap) };
    let options = MTLResourceOptions(u64_to_usize(info.options));
    let length = u64_to_usize(info.length);
    let buffer = heap.and_then(|heap| {
        placement_offset(p.offset).map_or_else(
            || heap.newBufferWithLength_options(length, options),
            // SAFETY: Metal checks the offset against the heap, as it did for the C.
            |offset| unsafe { heap.newBufferWithLength_options_offset(length, options, offset) },
        )
    });
    info.memory.ptr = if heap.is_some_and(|heap| heap.storageMode() == MTLStorageMode::Private) {
        0
    } else {
        buffer_memory(buffer.as_deref())
    };
    info.gpu_address = buffer.as_deref().map_or(0, MTLBuffer::gpuAddress);
    p.ret = ret_owned(buffer);
    STATUS_SUCCESS
}

/// Slot 142, `MTLHeap_newTexture`: a texture in a heap, at an offset unless `!0`.
///
/// # Safety
///
/// `args` is slot 142's params block ([`UnixcallMtlheapNewtexture`]), its handle a live heap.
#[unsafe(export_name = "_MTLHeap_newTexture")]
pub unsafe extern "C-unwind" fn heap_new_texture(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlheapNewtexture>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTTextureInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTTextureInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLHeap`, or the null handle.
    let heap = unsafe { obj::<ProtocolObject<dyn MTLHeap>>(p.heap) };
    let desc = MTLTextureDescriptor::new();
    fill_texture_descriptor(&desc, info);
    let texture = heap.and_then(|heap| {
        placement_offset(p.offset).map_or_else(
            || heap.newTextureWithDescriptor(&desc),
            // SAFETY: Metal checks the offset against the heap, as it did for the C.
            |offset| unsafe { heap.newTextureWithDescriptor_offset(&desc, offset) },
        )
    });
    info.gpu_resource_id = texture.as_deref().map_or(0, |texture| resource_id(texture.gpuResourceID()));
    info.mach_port = 0;
    p.ret = ret_owned(texture);
    STATUS_SUCCESS
}

/// Slot 143, `MTLDevice_newIndirectCommandBuffer`: an indirect command buffer.
///
/// Reads the four flags the C reads; the macOS 26 ones stay unread.
///
/// # Safety
///
/// `args` is slot 143's params block ([`UnixcallMtldeviceNewicb`]), its handle a live device.
#[unsafe(export_name = "_MTLDevice_newIndirectCommandBuffer")]
pub unsafe extern "C-unwind" fn new_indirect_command_buffer(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewicb>(args) };
    // SAFETY: the caller's contract: `info` is its `WMTIndirectCommandBufferInfo`.
    let Some(info) = (unsafe { mem_mut::<WMTIndirectCommandBufferInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's contract: a live `MTLDevice`, or the null handle.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let desc = MTLIndirectCommandBufferDescriptor::new();
    desc.setCommandTypes(MTLIndirectCommandType(info.r#type as usize));
    desc.setMaxFragmentBufferBindCount(usize::from(info.max_fragment_buffer_binding));
    desc.setMaxKernelBufferBindCount(usize::from(info.max_kernel_buffer_binding));
    // SAFETY: Metal checks the count when the buffer is made, as it did for the C.
    unsafe { desc.setMaxKernelThreadgroupMemoryBindCount(usize::from(info.max_kernel_threadgroup_memory_binding)) };
    // SAFETY: as above.
    unsafe { desc.setMaxMeshBufferBindCount(usize::from(info.max_mesh_buffer_binding)) };
    // SAFETY: as above.
    unsafe { desc.setMaxObjectBufferBindCount(usize::from(info.max_object_buffer_binding)) };
    // SAFETY: as above.
    unsafe { desc.setMaxObjectThreadgroupMemoryBindCount(usize::from(info.max_object_threadgroup_memory_binding)) };
    desc.setMaxVertexBufferBindCount(usize::from(info.max_vertex_buffer_binding));
    desc.setInheritBuffers(info.inherit_buffers() != 0);
    desc.setInheritPipelineState(info.inherit_pso() != 0);
    desc.setSupportDynamicAttributeStride(info.support_dynamic_attribute_stride() != 0);
    desc.setSupportRayTracing(info.support_ray_tracing() != 0);
    let icb = device.and_then(|device| {
        // SAFETY: Metal checks the count when the buffer is made, as it did for the C.
        unsafe {
            device.newIndirectCommandBufferWithDescriptor_maxCommandCount_options(
                &desc,
                u64_to_usize(p.max_count),
                MTLResourceOptions(u64_to_usize(p.options)),
            )
        }
    });
    info.gpu_resource_id = icb.as_deref().map_or(0, |icb| resource_id(icb.gpuResourceID()));
    p.ret = ret_owned(icb);
    STATUS_SUCCESS
}

/// Slot 145, `MTLTexture_getBytes`: a region of a texture into the guest's memory.
///
/// # Safety
///
/// `args` is slot 145's params block ([`UnixcallMtltextureReplaceregion`]), its handle a live
/// texture and its data room for the region's bytes.
#[unsafe(export_name = "_MTLTexture_getBytes")]
pub unsafe extern "C-unwind" fn texture_get_bytes(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtltextureReplaceregion>(args) };
    // SAFETY: the caller's contract: a live `MTLTexture`, or the null handle.
    let texture = unsafe { obj::<ProtocolObject<dyn MTLTexture>>(p.texture) };
    if let Some(texture) = texture {
        // SAFETY: the caller's contract: `data` has room for the region's bytes, which Metal writes.
        unsafe {
            texture.get_bytes(
                address(p.data.ptr),
                u64_to_usize(p.bytes_per_row),
                u64_to_usize(p.bytes_per_image),
                region(&p.origin, &p.size),
                u64_to_usize(p.level),
                u64_to_usize(p.slice),
            );
        }
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests;
