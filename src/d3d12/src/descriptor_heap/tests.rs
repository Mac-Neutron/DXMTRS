use core::{
    ffi::c_void,
    mem::{align_of, offset_of, size_of},
    ptr,
};

use dxmt_core::resource::{TextureViewDescriptor, TextureViewKey};

use crate::{
    abi::{
        objects::{
            BufferSrv, BufferUav, CbvCommonStorage, D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_NUM_TYPES,
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, DescriptorHeapDesc,
            SamplerGpuStorage, ShaderResourceViewDescPrefix, SrvBufferGpuStorage,
            SrvTexelBufferGpuStorage, SrvTextureGpuStorage, UavBufferGpuStorage,
            UavTexelBufferGpuStorage, UavTextureGpuStorage, UnorderedAccessViewDescPrefix,
        },
        vtbl::D3d12CpuDescriptorHandle,
    },
    com::{E_INVALIDARG, IID_ID3D12_DESCRIPTOR_HEAP, S_OK},
    descriptor_heap::{
        self, BufferSlice, EmbeddedDescriptorHandle, Heap, RenderTargetDesc,
        ShaderVisibleDescriptor,
    },
};

#[test]
fn handle_packs_as_cpp() {
    #[cfg(target_pointer_width = "64")]
    {
        let desc = DescriptorHeapDesc {
            r#type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            num_descriptors: 64,
            flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            node_mask: 0,
        };
        let mut raw: *mut c_void = ptr::null_mut();
        // SAFETY: Creating a non-shader-visible descriptor heap with null device.
        let hr = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut raw,
            )
        };
        assert_eq!(hr, S_OK);
        let heap_ptr = raw.cast::<Heap>().cast_const();
        let addr = heap_ptr.expose_provenance();
        assert_eq!(addr & 0x7, 0, "Heap allocation is at least 8-byte aligned");

        for idx in [0u32, 1, 7, 42, 0x12345, 0xFFFFF] {
            let handle = EmbeddedDescriptorHandle::pack(heap_ptr, idx);
            let expected = ((addr >> 3) & 0x1f) | ((idx as usize) << 5) | ((addr >> 8) << 25);
            assert_eq!(handle.ptr, expected, "packed bit layout for index {idx:#x}");
            let (unpacked_ptr, unpacked_idx) = EmbeddedDescriptorHandle::unpack(handle);
            assert_eq!(unpacked_ptr, heap_ptr);
            assert_eq!(unpacked_idx, idx);
        }

        // Incrementing handle.ptr by 32 advances the unpacked index by 1.
        let mut start = D3d12CpuDescriptorHandle { ptr: 0 };
        // SAFETY: `heap_ptr` is a live `Heap`.
        let vtbl = unsafe { (*heap_ptr).vtbl };
        // SAFETY: `raw` is a valid `Heap` and `start` is a writable `D3d12CpuDescriptorHandle`.
        unsafe {
            (vtbl.get_cpu_descriptor_handle_for_heap_start)(raw, (&raw mut start).cast());
        }
        assert_eq!(EmbeddedDescriptorHandle::unpack(start), (heap_ptr, 0));
        let slot3 = D3d12CpuDescriptorHandle {
            ptr: start.ptr + 3 * 32,
        };
        assert_eq!(EmbeddedDescriptorHandle::unpack(slot3), (heap_ptr, 3));

        // SAFETY: Release the heap created above.
        assert_eq!(unsafe { (vtbl.release)(raw) }, 0);
    }
}

#[test]
fn increments_are_32() {
    for ty in [
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
        D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
        D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
    ] {
        // SAFETY: `get_descriptor_handle_increment_size` does not dereference `this`.
        let inc =
            unsafe { descriptor_heap::get_descriptor_handle_increment_size(ptr::null_mut(), ty) };
        assert_eq!(inc, 32, "heap type {ty}");
    }
    // SAFETY: Invalid heap type returns 0.
    let bad = unsafe {
        descriptor_heap::get_descriptor_handle_increment_size(
            ptr::null_mut(),
            D3D12_DESCRIPTOR_HEAP_TYPE_NUM_TYPES,
        )
    };
    assert_eq!(bad, 0);
}

#[test]
fn gpu_entries_as_cpp() {
    assert_eq!(size_of::<DescriptorHeapDesc>(), 16);
    assert_eq!(align_of::<DescriptorHeapDesc>(), 4);
    assert_eq!(offset_of!(DescriptorHeapDesc, r#type), 0);
    assert_eq!(offset_of!(DescriptorHeapDesc, num_descriptors), 4);
    assert_eq!(offset_of!(DescriptorHeapDesc, flags), 8);
    assert_eq!(offset_of!(DescriptorHeapDesc, node_mask), 12);

    assert_eq!(size_of::<SrvTextureGpuStorage>(), 32);
    assert_eq!(align_of::<SrvTextureGpuStorage>(), 8);
    assert_eq!(offset_of!(SrvTextureGpuStorage, resource_id), 0);
    assert_eq!(offset_of!(SrvTextureGpuStorage, metadata), 8);
    assert_eq!(offset_of!(SrvTextureGpuStorage, atomic_resource_id), 16);
    assert_eq!(offset_of!(SrvTextureGpuStorage, padding), 24);
    assert_eq!(size_of::<UavTextureGpuStorage>(), 32);

    assert_eq!(size_of::<UavTexelBufferGpuStorage>(), 32);
    assert_eq!(align_of::<UavTexelBufferGpuStorage>(), 8);
    assert_eq!(offset_of!(UavTexelBufferGpuStorage, resource_id), 0);
    assert_eq!(offset_of!(UavTexelBufferGpuStorage, metadata), 8);
    assert_eq!(offset_of!(UavTexelBufferGpuStorage, atomic_resource_id), 16);
    assert_eq!(offset_of!(UavTexelBufferGpuStorage, buffer_address), 24);
    assert_eq!(size_of::<SrvTexelBufferGpuStorage>(), 32);

    assert_eq!(size_of::<UavBufferGpuStorage>(), 32);
    assert_eq!(align_of::<UavBufferGpuStorage>(), 8);
    assert_eq!(offset_of!(UavBufferGpuStorage, pointer), 0);
    assert_eq!(offset_of!(UavBufferGpuStorage, metadata), 8);
    assert_eq!(offset_of!(UavBufferGpuStorage, counter_pointer), 16);
    assert_eq!(offset_of!(UavBufferGpuStorage, padding), 24);
    assert_eq!(size_of::<SrvBufferGpuStorage>(), 32);

    assert_eq!(size_of::<CbvCommonStorage>(), 16);
    assert_eq!(align_of::<CbvCommonStorage>(), 8);
    assert_eq!(offset_of!(CbvCommonStorage, address), 0);
    assert_eq!(offset_of!(CbvCommonStorage, size), 8);

    assert_eq!(size_of::<SamplerGpuStorage>(), 32);
    assert_eq!(align_of::<SamplerGpuStorage>(), 8);
    assert_eq!(offset_of!(SamplerGpuStorage, sampler), 0);
    assert_eq!(offset_of!(SamplerGpuStorage, cube_sampler), 8);
    assert_eq!(offset_of!(SamplerGpuStorage, metadata), 16);
    assert_eq!(offset_of!(SamplerGpuStorage, padding), 24);

    assert_eq!(size_of::<BufferSrv>(), 24);
    assert_eq!(size_of::<ShaderResourceViewDescPrefix>(), 40);
    assert_eq!(offset_of!(ShaderResourceViewDescPrefix, buffer), 16);
    assert_eq!(size_of::<BufferUav>(), 32);
    assert_eq!(size_of::<UnorderedAccessViewDescPrefix>(), 40);
    assert_eq!(offset_of!(UnorderedAccessViewDescPrefix, buffer), 8);

    #[cfg(target_pointer_width = "64")]
    {
        let desc = DescriptorHeapDesc {
            r#type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            num_descriptors: 8,
            flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            node_mask: 0,
        };
        let mut raw: *mut c_void = ptr::null_mut();
        // SAFETY: Creating a non-shader-visible CBV/SRV/UAV heap.
        let hr = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut raw,
            )
        };
        assert_eq!(hr, S_OK);
        let heap = raw.cast::<Heap>();

        // Slot 0: CBV
        let h0 = EmbeddedDescriptorHandle::pack(heap, 0);
        // SAFETY: `h0` points to slot 0 of `heap`.
        let hr_cbv = unsafe { descriptor_heap::write_cbv(h0, 0xdead_beef_0000, 256) };
        assert_eq!(hr_cbv, S_OK);

        // Slot 1: null structured buffer SRV with stride 16, count 32
        let h1 = EmbeddedDescriptorHandle::pack(heap, 1);
        let slice_srv = BufferSlice {
            byte_offset: 64,
            byte_length: 512,
            first_element: 4,
            element_count: 32,
            byte_stride: 16,
        };
        // SAFETY: `h1` points to slot 1 of `heap`.
        let hr_srv = unsafe { descriptor_heap::write_srv_buffer(h1, None, slice_srv) };
        assert_eq!(hr_srv, S_OK);

        // Slot 2: null raw buffer UAV
        let h2 = EmbeddedDescriptorHandle::pack(heap, 2);
        let slice_uav = BufferSlice {
            byte_offset: 0,
            byte_length: 128,
            first_element: 0,
            element_count: 32,
            byte_stride: 4,
        };
        // SAFETY: `h2` points to slot 2 of `heap`.
        let hr_uav = unsafe { descriptor_heap::write_uav_buffer(h2, None, slice_uav, None, 0) };
        assert_eq!(hr_uav, S_OK);

        // Verify the 32-byte GPU words stored in `samplers` (`[u64; 4]` per slot).
        // SAFETY: `heap` is live.
        let gpu_words = unsafe { (*heap).samplers.lock().unwrap().clone() };
        assert_eq!(gpu_words[0], [0xdead_beef_0000, 256, 0, 0]);
        assert_eq!(gpu_words[1], [0, 0, 0, 0]);
        assert_eq!(gpu_words[2], [0, 0, 0, 0]);

        // SAFETY: `heap` is live.
        let vtbl = unsafe { (*heap).vtbl };
        // SAFETY: Release `heap`.
        assert_eq!(unsafe { (vtbl.release)(raw) }, 0);
    }
}

#[test]
fn rtv_heap_refuses_shader_visible() {
    for rtv_or_dsv in [
        D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
    ] {
        let bad_desc = DescriptorHeapDesc {
            r#type: rtv_or_dsv,
            num_descriptors: 4,
            flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            node_mask: 0,
        };
        let mut out: *mut c_void = ptr::dangling_mut();
        // SAFETY: Testing shader-visible rejection on RTV/DSV heaps.
        let hr = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const bad_desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut out,
            )
        };
        assert_eq!(hr, E_INVALIDARG);
        assert!(out.is_null());
    }

    // NumDescriptors > 0xFFFFF is rejected with E_INVALIDARG.
    let too_many = DescriptorHeapDesc {
        r#type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
        num_descriptors: 0x10_0000,
        flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        node_mask: 0,
    };
    let mut out: *mut c_void = ptr::dangling_mut();
    // SAFETY: Testing `NumDescriptors` limit validation.
    let hr_many = unsafe {
        descriptor_heap::create_descriptor_heap(
            ptr::null_mut(),
            (&raw const too_many).cast(),
            ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
            &raw mut out,
        )
    };
    assert_eq!(hr_many, E_INVALIDARG);
    assert!(out.is_null());
}

#[test]
fn copy_descriptors_as_cpp() {
    #[cfg(target_pointer_width = "64")]
    {
        let desc = DescriptorHeapDesc {
            r#type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            num_descriptors: 8,
            flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            node_mask: 0,
        };
        let mut src_raw: *mut c_void = ptr::null_mut();
        let mut dst_raw: *mut c_void = ptr::null_mut();
        // SAFETY: Creating source CBV/SRV/UAV heap.
        let hr_src = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut src_raw,
            )
        };
        assert_eq!(hr_src, S_OK);
        // SAFETY: Creating destination CBV/SRV/UAV heap.
        let hr_dst = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut dst_raw,
            )
        };
        assert_eq!(hr_dst, S_OK);
        let src_heap = src_raw.cast::<Heap>();
        let dst_heap = dst_raw.cast::<Heap>();

        // Populate slots 0..3 in `src_heap` with distinct CBVs.
        for i in 0..3u32 {
            let h = EmbeddedDescriptorHandle::pack(src_heap, i);
            // SAFETY: `h` is a valid handle into `src_heap`.
            let hr_cbv = unsafe {
                descriptor_heap::write_cbv(h, 0x1000_0000 + u64::from(i) * 0x100, 64 * (i + 1))
            };
            assert_eq!(hr_cbv, S_OK);
        }

        // CopyDescriptorsSimple: copy 2 descriptors from src[0..2] to dst[1..3].
        // SAFETY: `src_heap` and `dst_heap` are live CBV/SRV/UAV heaps.
        unsafe {
            descriptor_heap::copy_descriptors_simple(
                ptr::null_mut(),
                2,
                EmbeddedDescriptorHandle::pack(dst_heap, 1),
                EmbeddedDescriptorHandle::pack(src_heap, 0),
                D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            );
        }

        // SAFETY: `dst_heap` is live.
        let d1 = unsafe { (*dst_heap).descriptor(1) };
        // SAFETY: `dst_heap` is live.
        let d2 = unsafe { (*dst_heap).descriptor(2) };
        assert!(matches!(
            d1,
            ShaderVisibleDescriptor::ConstantBuffer {
                address: 0x1000_0000,
                size: 64
            }
        ));
        assert!(matches!(
            d2,
            ShaderVisibleDescriptor::ConstantBuffer {
                address: 0x1000_0100,
                size: 128
            }
        ));

        // CopyDescriptors: scatter/gather 3 descriptors from src ranges [0..2, 2..3] into dst
        // ranges [4..5, 5..7].
        let src_starts = [
            EmbeddedDescriptorHandle::pack(src_heap, 0),
            EmbeddedDescriptorHandle::pack(src_heap, 2),
        ];
        let src_sizes = [2u32, 1u32];
        let dst_starts = [
            EmbeddedDescriptorHandle::pack(dst_heap, 4),
            EmbeddedDescriptorHandle::pack(dst_heap, 5),
        ];
        let dst_sizes = [1u32, 2u32];
        // SAFETY: `src_starts`, `src_sizes`, `dst_starts`, and `dst_sizes` are valid arrays.
        unsafe {
            descriptor_heap::copy_descriptors(
                ptr::null_mut(),
                2,
                dst_starts.as_ptr().cast(),
                dst_sizes.as_ptr(),
                2,
                src_starts.as_ptr().cast(),
                src_sizes.as_ptr(),
                D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            );
        }

        // SAFETY: `dst_heap` is live.
        let gpu_words = unsafe { (*dst_heap).samplers.lock().unwrap().clone() };
        assert_eq!(gpu_words[4], [0x1000_0000, 64, 0, 0]);
        assert_eq!(gpu_words[5], [0x1000_0100, 128, 0, 0]);
        assert_eq!(gpu_words[6], [0x1000_0200, 192, 0, 0]);

        // Also test RTV copy_descriptors_simple.
        let rtv_desc = DescriptorHeapDesc {
            r#type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            num_descriptors: 4,
            flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            node_mask: 0,
        };
        let mut rtv_raw: *mut c_void = ptr::null_mut();
        // SAFETY: Creating an RTV descriptor heap.
        let hr_rtv_create = unsafe {
            descriptor_heap::create_descriptor_heap(
                ptr::null_mut(),
                (&raw const rtv_desc).cast(),
                ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
                &raw mut rtv_raw,
            )
        };
        assert_eq!(hr_rtv_create, S_OK);
        let rtv_heap = rtv_raw.cast::<Heap>();
        let dummy_view: TextureViewKey = TextureViewDescriptor {
            format: 80,
            texture_type: 2,
            first_mip: 0,
            mip_count: 1,
            first_slice: 0,
            array_size: 1,
        }
        .key();
        // SAFETY: Writing RTV into slot 0 of `rtv_heap`.
        let hr_rtv_write = unsafe {
            descriptor_heap::write_rtv(
                EmbeddedDescriptorHandle::pack(rtv_heap, 0),
                RenderTargetDesc {
                    texture: None,
                    view: dummy_view,
                    width: 1920,
                    height: 1080,
                    depth_plane: 1,
                    render_target_array_length: 1,
                    flags: 0,
                },
            )
        };
        assert_eq!(hr_rtv_write, S_OK);
        // SAFETY: Copying RTV slot 0 to slot 2 within `rtv_heap`.
        unsafe {
            descriptor_heap::copy_descriptors_simple(
                ptr::null_mut(),
                1,
                EmbeddedDescriptorHandle::pack(rtv_heap, 2),
                EmbeddedDescriptorHandle::pack(rtv_heap, 0),
                D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            );
        }
        // SAFETY: Reading back RTV slot 2.
        let copied_rtv =
            unsafe { descriptor_heap::render_target(EmbeddedDescriptorHandle::pack(rtv_heap, 2)) }
                .expect("copied RTV exists");
        assert_eq!(copied_rtv.width, 1920);
        assert_eq!(copied_rtv.height, 1080);

        // SAFETY: `rtv_heap` is live.
        let rtv_vtbl = unsafe { (*rtv_heap).vtbl };
        // SAFETY: Release `rtv_heap`.
        assert_eq!(unsafe { (rtv_vtbl.release)(rtv_raw) }, 0);
        // SAFETY: `dst_heap` is live.
        let dst_vtbl = unsafe { (*dst_heap).vtbl };
        // SAFETY: Release `dst_heap`.
        assert_eq!(unsafe { (dst_vtbl.release)(dst_raw) }, 0);
        // SAFETY: `src_heap` is live.
        let src_vtbl = unsafe { (*src_heap).vtbl };
        // SAFETY: Release `src_heap`.
        assert_eq!(unsafe { (src_vtbl.release)(src_raw) }, 0);
    }
}
