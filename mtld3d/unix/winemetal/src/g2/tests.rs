use core::mem::size_of;

use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice, MTLResourceOptions};

use super::*;
use crate::ffi::Handler;

/// A sampler description with the filters of a trilinear, non-comparison sampler.
const fn trilinear(max_anisotroy: u32) -> WMTSamplerInfo {
    WMTSamplerInfo {
        min_filter: WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
        mag_filter: WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
        mip_filter: WMT_SAMPLER_MIP_FILTER_LINEAR,
        r_address_mode: 0,
        s_address_mode: 0,
        t_address_mode: 0,
        border_color: 0,
        compare_function: WMT_COMPARE_FUNCTION_NEVER,
        lod_min_clamp: 0.0,
        lod_max_clamp: 0.0,
        max_anisotroy,
        normalized_coords: 1,
        lod_average: 0,
        support_argument_buffers: 0,
        gpu_resource_id: 0,
    }
}

/// A shared buffer of `length` bytes, every byte `fill`.
fn shared_buffer(length: usize, fill: u8) -> Retained<ProtocolObject<dyn MTLBuffer>> {
    let device = MTLCreateSystemDefaultDevice().expect("a Metal device");
    let buffer = device
        .newBufferWithLength_options(length, MTLResourceOptions::StorageModeShared)
        .expect("a buffer");
    // SAFETY: a shared buffer's contents are `length` writable bytes.
    unsafe { core::ptr::write_bytes(buffer.contents().as_ptr().cast::<u8>(), fill, length) };
    buffer
}

/// A shared buffer's bytes.
fn bytes(buffer: &ProtocolObject<dyn MTLBuffer>, length: usize) -> Vec<u8> {
    // SAFETY: a shared buffer's contents are `length` readable bytes, and nothing writes them here.
    unsafe { core::slice::from_raw_parts(buffer.contents().as_ptr().cast::<u8>(), length) }.to_vec()
}

/// Slot 107 on `buffer` (a handle, possibly null) with `data` (a pointer, possibly null).
fn update(buffer: u64, offset: u64, data: *const u8, length: u64) -> i32 {
    let mut p = UnixcallMtlbufferUpdatecontents {
        buffer,
        offset,
        data: winemetal_wire::MemPtr { ptr: data.expose_provenance() as u64 },
        length,
    };
    // SAFETY: a params block for slot 107 whose handle is null or a live buffer, and whose data is null or
    // `length` readable bytes.
    unsafe { buffer_update_contents(core::ptr::from_mut(&mut p).cast()) }
}

#[test]
fn forced_anisotropy_raises_a_trilinear_sampler() {
    assert_eq!(max_anisotropy(&trilinear(1), 8), 8);
    assert_eq!(max_anisotropy(&trilinear(7), 8), 8);
}

#[test]
fn forced_anisotropy_never_lowers_one() {
    assert_eq!(max_anisotropy(&trilinear(8), 8), 8);
    assert_eq!(max_anisotropy(&trilinear(16), 8), 16);
}

#[test]
fn forced_anisotropy_of_zero_changes_nothing() {
    assert_eq!(max_anisotropy(&trilinear(3), 0), 3);
}

#[test]
fn forced_anisotropy_leaves_other_samplers() {
    let mut info = trilinear(1);
    info.min_filter = 0;
    assert_eq!(max_anisotropy(&info, 8), 1);
    let mut info = trilinear(1);
    info.mag_filter = 0;
    assert_eq!(max_anisotropy(&info, 8), 1);
    let mut info = trilinear(1);
    info.mip_filter = 1;
    assert_eq!(max_anisotropy(&info, 8), 1);
    let mut info = trilinear(1);
    info.compare_function = 1;
    assert_eq!(max_anisotropy(&info, 8), 1);
}

#[test]
fn all_ones_offset_is_no_placement() {
    assert_eq!(placement_offset(u64::MAX), None);
    assert_eq!(placement_offset(0), Some(0));
    assert_eq!(placement_offset(65536), Some(65536));
    assert_eq!(placement_offset(u64::MAX - 1), Some(usize::MAX - 1));
}

#[test]
fn resource_id_is_one_word() {
    assert_eq!(size_of::<MTLResourceID>(), size_of::<u64>());
    let device = MTLCreateSystemDefaultDevice().expect("a Metal device");
    let desc = MTLSamplerDescriptor::new();
    desc.setSupportArgumentBuffers(true);
    let sampler = device.newSamplerStateWithDescriptor(&desc).expect("a sampler");
    assert_ne!(resource_id(sampler.gpuResourceID()), 0);
}

#[test]
fn handlers_have_the_handler_type() {
    let handlers: [Handler; 28] = [
        new_buffer,
        new_sampler_state,
        new_depth_stencil_state,
        new_texture,
        buffer_new_texture,
        texture_new_texture_view,
        texture_pixel_format,
        texture_width,
        texture_height,
        texture_depth,
        texture_array_length,
        texture_mipmap_level_count,
        texture_replace_region,
        buffer_did_modify_range,
        buffer_update_contents,
        new_residency_set,
        residency_set_add_allocations,
        residency_set_remove_allocations,
        residency_set_remove_all_allocations,
        residency_set_commit,
        queue_add_residency_set,
        new_heap,
        heap_buffer_size_and_align,
        heap_texture_size_and_align,
        heap_new_buffer,
        heap_new_texture,
        new_indirect_command_buffer,
        texture_get_bytes,
    ];
    assert_eq!(handlers.len(), 28);
}

#[test]
fn getters_of_the_null_handle_answer_zero() {
    let getters: [Handler; 6] = [
        texture_pixel_format,
        texture_width,
        texture_height,
        texture_depth,
        texture_array_length,
        texture_mipmap_level_count,
    ];
    for getter in getters {
        let mut p = UnixcallGenericObjUint64Ret { handle: 0, ret: 0xa5a5_a5a5_a5a5_a5a5 };
        // SAFETY: a params block for a getter slot whose handle is null.
        assert_eq!(unsafe { getter(core::ptr::from_mut(&mut p).cast()) }, 0);
        assert_eq!(p.ret, 0);
    }
}

#[test]
fn update_contents_copies_at_the_offset() {
    let buffer = shared_buffer(32, 0xa5);
    let handle = Retained::as_ptr(&buffer).expose_provenance() as u64;
    let data = [1_u8, 2, 3, 4];
    assert_eq!(update(handle, 8, data.as_ptr(), 4), 0);
    let mut want = vec![0xa5_u8; 32];
    want[8..12].copy_from_slice(&data);
    assert_eq!(bytes(&buffer, 32), want);
}

#[test]
fn update_contents_of_no_bytes_copies_nothing() {
    let buffer = shared_buffer(16, 0xa5);
    let handle = Retained::as_ptr(&buffer).expose_provenance() as u64;
    assert_eq!(update(handle, 4, core::ptr::null(), 0), 0);
    assert_eq!(bytes(&buffer, 16), vec![0xa5_u8; 16]);
}

#[test]
fn update_contents_of_a_null_buffer_writes_nothing() {
    let data = [1_u8, 2, 3, 4];
    assert_eq!(update(0, 0, data.as_ptr(), 4), 0);
}

#[test]
fn update_contents_without_data_writes_nothing() {
    let buffer = shared_buffer(16, 0xa5);
    let handle = Retained::as_ptr(&buffer).expose_provenance() as u64;
    assert_eq!(update(handle, 0, core::ptr::null(), 4), 0);
    assert_eq!(bytes(&buffer, 16), vec![0xa5_u8; 16]);
}

#[cfg(feature = "native")]
#[test]
fn hook_memory_is_whole_pages() {
    assert_eq!(page_rounded(0), 0);
    assert_eq!(page_rounded(1), 16384);
    assert_eq!(page_rounded(16384), 16384);
    assert_eq!(page_rounded(16385), 32768);
    assert_eq!(page_rounded(u64::MAX), 0);
}

#[cfg(feature = "native")]
mod native {
    use core::ffi::c_void;
    use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use objc2_metal::MTLCreateSystemDefaultDevice;
    use winemetal_wire::{MemPtr, WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED};

    use super::super::*;
    use super::shared_buffer;

    static ALLOCS: AtomicUsize = AtomicUsize::new(0);
    static FREES: AtomicUsize = AtomicUsize::new(0);
    static LENGTH: AtomicU64 = AtomicU64::new(0);
    const CONTEXT: usize = 0x1234;

    unsafe extern "C-unwind" fn alloc(length: u64, ctx: *mut c_void) -> *mut c_void {
        let mut memory = core::ptr::null_mut();
        // SAFETY: `memory` is a writable pointer; the alignment is a power of two.
        let failed = unsafe { libc::posix_memalign(&raw mut memory, 16384, u64_to_usize(length)) } != 0;
        if failed || ctx.expose_provenance() != CONTEXT {
            return core::ptr::null_mut();
        }
        LENGTH.store(length, Ordering::SeqCst);
        ALLOCS.fetch_add(1, Ordering::SeqCst);
        memory
    }

    unsafe extern "C-unwind" fn free(memory: *mut c_void, ctx: *mut c_void) {
        if ctx.expose_provenance() == CONTEXT {
            FREES.fetch_add(1, Ordering::SeqCst);
        }
        // SAFETY: `memory` came from `alloc`'s `posix_memalign`.
        unsafe { libc::free(memory) };
    }

    /// Slot 18 on `device` for a buffer of `length` bytes with `options` and the guest's `memory`.
    fn new(device: u64, options: u64, memory: u64) -> (UnixcallMtldeviceNewbuffer, WMTBufferInfo) {
        let mut info = WMTBufferInfo { length: 100, options, memory: MemPtr { ptr: memory }, gpu_address: 0 };
        let mut p = UnixcallMtldeviceNewbuffer {
            device,
            info: MemPtr { ptr: core::ptr::from_mut(&mut info).expose_provenance() as u64 },
            ret: 0,
        };
        // SAFETY: a params block for slot 18 whose handle is a live device and whose info is live.
        assert_eq!(unsafe { new_buffer(core::ptr::from_mut(&mut p).cast()) }, 0);
        (p, info)
    }

    #[test]
    fn hooked_buffer_lives_in_the_hosts_pages() {
        let device = MTLCreateSystemDefaultDevice().expect("a Metal device");
        let handle = Retained::as_ptr(&device).expose_provenance() as u64;
        assert_eq!(memory_hooked(), 0);
        set_memory_hooks(Some(alloc), Some(free), core::ptr::with_exposed_provenance_mut(CONTEXT));
        assert_eq!(memory_hooked(), 1);

        let (p, info) = new(handle, WMT_RESOURCE_STORAGE_MODE_SHARED, 0);
        assert_eq!(ALLOCS.load(Ordering::SeqCst), 1);
        assert_eq!(LENGTH.load(Ordering::SeqCst), 16384);
        assert_ne!(info.memory.ptr, 0);
        let ptr = core::ptr::with_exposed_provenance_mut::<ProtocolObject<dyn MTLBuffer>>(u64_to_usize(p.ret));
        // SAFETY: the handler gave the guest a reference to a live buffer; this takes it over.
        let buffer = unsafe { Retained::from_raw(ptr) }.expect("a buffer");
        assert_eq!(buffer.contents().as_ptr().expose_provenance() as u64, info.memory.ptr);
        assert_ne!(info.gpu_address, 0);

        // Private memory is Metal's, and memory the guest brought goes under the buffer as it is.
        let (private, info) = new(handle, WMT_RESOURCE_STORAGE_MODE_PRIVATE, 0);
        assert_ne!(private.ret, 0);
        assert_eq!(info.memory.ptr, 0);
        let own = shared_buffer(16384, 0);
        let (brought, info) = new(handle, WMT_RESOURCE_STORAGE_MODE_SHARED, own.contents().as_ptr().expose_provenance() as u64);
        assert_ne!(brought.ret, 0);
        assert_eq!(ALLOCS.load(Ordering::SeqCst), 1);
        assert_eq!(info.memory.ptr, own.contents().as_ptr().expose_provenance() as u64);

        // Releasing the buffer gives the pages back.
        drop(buffer);
        let until = Instant::now() + Duration::from_secs(2);
        while FREES.load(Ordering::SeqCst) == 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(FREES.load(Ordering::SeqCst), 1);
        set_memory_hooks(None, None, core::ptr::null_mut());
        assert_eq!(memory_hooked(), 0);
    }
}
