//! Resources, heaps and residency sets.

use winemetal_wire::{
    UnixcallGenericObjNoret, UnixcallGenericObjObjNoret, UnixcallMtlbufferNewtexture,
    UnixcallMtldeviceHeapbuffersizeandalign, UnixcallMtldeviceHeaptexturesizeandalign,
    UnixcallMtldeviceNewbuffer, UnixcallMtldeviceNewdepthstencilstate, UnixcallMtldeviceNewheap,
    UnixcallMtldeviceNewicb, UnixcallMtldeviceNewresidencyset, UnixcallMtldeviceNewsamplerstate,
    UnixcallMtldeviceNewtexture, UnixcallMtlheapNewbuffer, UnixcallMtlheapNewtexture,
    UnixcallMtlresidencysetAddallocations, UnixcallMtltextureNewtextureview, WMTBufferInfo,
    WMTDepthStencilInfo, WMTHeapInfo, WMTIndirectCommandBufferInfo, WMTSamplerInfo,
    WMTSizeAndAlign, WMTTextureInfo, WMTTextureSwizzleChannels, slot,
};

use crate::transport::{call, mem_ptr, write_if};

exports! {
    fn MTLDevice_newBuffer(device: u64, info: *mut WMTBufferInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_BUFFER, |p: &mut UnixcallMtldeviceNewbuffer| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_newSamplerState(device: u64, info: *mut WMTSamplerInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_SAMPLER_STATE, |p: &mut UnixcallMtldeviceNewsamplerstate| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_newDepthStencilState(device: u64, info: *const WMTDepthStencilInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_DEPTH_STENCIL_STATE, |p: &mut UnixcallMtldeviceNewdepthstencilstate| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_newTexture(device: u64, info: *mut WMTTextureInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_TEXTURE, |p: &mut UnixcallMtldeviceNewtexture| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLBuffer_newTexture(buffer: u64, info: *mut WMTTextureInfo, offset: u64, bytes_per_row: u64) -> u64 {
        call(slot::MTL_BUFFER_NEW_TEXTURE, |p: &mut UnixcallMtlbufferNewtexture| {
            p.buffer = buffer;
            p.info = mem_ptr(info);
            p.offset = offset;
            p.bytes_per_row = bytes_per_row;
        })
        .ret
    }

    fn MTLTexture_newTextureView(
        texture: u64,
        format: u32,
        texture_type: u32,
        level_start: u16,
        level_count: u16,
        slice_start: u16,
        slice_count: u16,
        swizzle: WMTTextureSwizzleChannels,
        out_gpu_resource_id: *mut u64,
    ) -> u64 {
        let p = call(slot::MTL_TEXTURE_NEW_TEXTURE_VIEW, |p: &mut UnixcallMtltextureNewtextureview| {
            p.texture = texture;
            p.format = format;
            p.texture_type = texture_type;
            p.level_start = level_start;
            p.level_count = level_count;
            p.slice_start = slice_start;
            p.slice_count = slice_count;
            p.swizzle = swizzle;
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { out_gpu_resource_id.write(p.gpu_resource_id) };
        p.ret
    }

    fn MTLDevice_newSharedTexture(device: u64, info: *mut WMTTextureInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_SHARED_TEXTURE, |p: &mut UnixcallMtldeviceNewtexture| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_newResidencySet(device: u64, init_capacity: u64, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_RESIDENCY_SET, |p: &mut UnixcallMtldeviceNewresidencyset| {
            p.device = device;
            p.init_capacity = init_capacity;
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_set
    }

    fn MTLResidencySet_addAllocations(residency_set: u64, allocations: *const u64, count: u64) {
        call(slot::MTL_RESIDENCY_SET_ADD_ALLOCATIONS, |p: &mut UnixcallMtlresidencysetAddallocations| {
            p.set = residency_set;
            p.allocations = mem_ptr(allocations);
            p.count = count;
        });
    }

    fn MTLResidencySet_removeAllocations(residency_set: u64, allocations: *const u64, count: u64) {
        call(slot::MTL_RESIDENCY_SET_REMOVE_ALLOCATIONS, |p: &mut UnixcallMtlresidencysetAddallocations| {
            p.set = residency_set;
            p.allocations = mem_ptr(allocations);
            p.count = count;
        });
    }

    fn MTLResidencySet_removeAllAllocations(residency_set: u64) {
        call(slot::MTL_RESIDENCY_SET_REMOVE_ALL_ALLOCATIONS, |p: &mut UnixcallGenericObjNoret| {
            p.handle = residency_set;
        });
    }

    fn MTLResidencySet_commit(residency_set: u64) {
        call(slot::MTL_RESIDENCY_SET_COMMIT, |p: &mut UnixcallGenericObjNoret| p.handle = residency_set);
    }

    fn MTLCommandQueue_addResidencySet(queue: u64, residency_set: u64) {
        call(slot::MTL_COMMAND_QUEUE_ADD_RESIDENCY_SET, |p: &mut UnixcallGenericObjObjNoret| {
            p.handle = queue;
            p.arg = residency_set;
        });
    }

    fn MTLDevice_newHeap(device: u64, info: *const WMTHeapInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_HEAP, |p: &mut UnixcallMtldeviceNewheap| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_heapBufferSizeAndAlign(device: u64, length: u64, options: u64) -> WMTSizeAndAlign {
        let p = call(slot::MTL_DEVICE_HEAP_BUFFER_SIZE_AND_ALIGN, |p: &mut UnixcallMtldeviceHeapbuffersizeandalign| {
            p.device = device;
            p.length = length;
            p.options = options;
        });
        WMTSizeAndAlign { size: p.ret_size, align: p.ret_align }
    }

    fn MTLDevice_heapTextureSizeAndAlign(device: u64, info: *mut WMTTextureInfo) -> WMTSizeAndAlign {
        let p = call(
            slot::MTL_DEVICE_HEAP_TEXTURE_SIZE_AND_ALIGN,
            |p: &mut UnixcallMtldeviceHeaptexturesizeandalign| {
                p.device = device;
                p.info = mem_ptr(info);
            },
        );
        WMTSizeAndAlign { size: p.ret_size, align: p.ret_align }
    }

    fn MTLHeap_newBuffer(heap: u64, info: *mut WMTBufferInfo, offset: u64) -> u64 {
        call(slot::MTL_HEAP_NEW_BUFFER, |p: &mut UnixcallMtlheapNewbuffer| {
            p.heap = heap;
            p.info = mem_ptr(info);
            p.offset = offset;
        })
        .ret
    }

    fn MTLHeap_newTexture(heap: u64, info: *mut WMTTextureInfo, offset: u64) -> u64 {
        call(slot::MTL_HEAP_NEW_TEXTURE, |p: &mut UnixcallMtlheapNewtexture| {
            p.heap = heap;
            p.info = mem_ptr(info);
            p.offset = offset;
        })
        .ret
    }

    fn MTLDevice_newIndirectCommandBuffer(
        device: u64,
        info: *mut WMTIndirectCommandBufferInfo,
        max_count: u64,
        options: u64,
    ) -> u64 {
        call(slot::MTL_DEVICE_NEW_INDIRECT_COMMAND_BUFFER, |p: &mut UnixcallMtldeviceNewicb| {
            p.device = device;
            p.info = mem_ptr(info);
            p.max_count = max_count;
            p.options = options;
        })
        .ret
    }
}
