//! Unit tests for command-list state setters and index buffer cache invalidation.

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::{Arc, Mutex};

use dxmt_core::resource::{
    Buffer, BufferAllocationFlags, Texture, TextureAllocationFlags, TextureViewDescriptor,
};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_STORE,
        WMT_TEXTURE_TYPE2D, WMTTextureInfo,
    },
};

use super::{
    ext_query_interface, ext_release, ia_set_index_buffer, om_set_front_and_back_stencil_ref,
    om_set_render_targets, query_interface, reset_state, rs_set_depth_bias, set_pipeline_state,
};
use crate::{
    abi::{
        ext::{IID_D3D12_COMMAND_LIST_EXT, IID_IMTL_D3D12_COMMAND_LIST_EXT1},
        list_state::D3d12IndexBufferView,
        objects::D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
        vtbl::{D3d12CpuDescriptorHandle, ID3D12DescriptorHeapVtbl, ID3D12PipelineStateVtbl},
    },
    com::{
        IID_ID3D12_GRAPHICS_COMMAND_LIST8, IID_ID3D12_GRAPHICS_COMMAND_LIST9,
        IID_ID3D12_GRAPHICS_COMMAND_LIST10, IID_IUNKNOWN, S_OK, add_ref_com, release_com,
    },
    descriptor_heap::{EmbeddedDescriptorHandle, Heap, RenderTargetDesc},
    device::{Device, residency},
    list::{DirtyFlags, LIST_VTBL, List},
    pipeline::Graphics,
};

fn make_test_texture(pixel_format: u32, sample_count: u32) -> Arc<Texture> {
    let info = WMTTextureInfo {
        pixel_format,
        width: 64,
        height: 64,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D & 0xff) | (1 << 8) | ((sample_count & 0xff) << 16),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    // SAFETY: `crate::tests` stubs `NSObject_retain`/`NSObject_release` for test `Obj` handles.
    let obj = unsafe { Obj::adopt(1) }.expect("non-zero fake handle");
    let tex = Arc::new(Texture::new(info, &obj));
    let alloc = tex.allocate(TextureAllocationFlags::SHARED);
    let _ = tex.rename(alloc);
    tex
}

fn make_render_target_heap(entries: Vec<RenderTargetDesc>) -> Box<Heap> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12DescriptorHeapVtbl` and consists of non-null
    // function pointers; tests never dispatch through heap vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12DescriptorHeapVtbl>() };
    Box::new(Heap {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        heap_type: D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
        num_descriptors: u32::try_from(entries.len()).unwrap_or(0),
        flags: 0,
        gpu_buffer: None,
        gpu_mapped_ptr: ptr::null_mut(),
        gpu_base_address: 0,
        cpu_words_ptr: ptr::null_mut(),
        has_cpu_descriptors: AtomicBool::new(false),
        cpu_descriptors: Mutex::new(Vec::new()),
        samplers: Mutex::new(Vec::new()),
        render_targets: Mutex::new(entries),
    })
}

#[test]
fn ia_set_index_buffer_invalidates_ib_cache_on_va_generation_bump() {
    let dev_box = Device::new_dummy();
    let dev_ptr = Box::into_raw(dev_box);
    let mut list = List::new_closed(dev_ptr);

    // SAFETY: `crate::tests` provides no-op `NSObject_retain`/`NSObject_release` stubs.
    let mtl = unsafe { Obj::adopt(1) }.expect("non-zero fake handle");
    let buf = Arc::new(Buffer::new(4096, &mtl));
    let alloc = buf.allocate(BufferAllocationFlags::empty());
    let _ = buf.rename(alloc);

    if let Some(current) = buf.current() {
        let mtl_buf = current.buffer();
        drop(current);
        let va = 0x1000_0000_u64;

        // Test stubs allocate `MTLBuffer` with `gpu_address == 0`; insert at a non-zero GPU VA and
        // bump `va_generation` so `ia_set_index_buffer` exercises live lookup and cache updates.
        // SAFETY: `dev_ptr` points to the dummy device allocated above.
        let dev = unsafe { &*dev_ptr };
        dev.residency
            .va_map
            .write()
            .expect("unpoisoned va_map lock")
            .insert(va, vec![Arc::clone(&buf)]);
        dev.residency.va_generation.fetch_add(1, Ordering::Release);

        let ibv = D3d12IndexBufferView {
            buffer_location: va,
            size_in_bytes: 256,
            format: 57,
        };
        let list_ptr: *mut c_void = (&raw mut *list).cast();
        // SAFETY: `list_ptr` and `&ibv` point to valid test objects.
        unsafe { ia_set_index_buffer(list_ptr, (&raw const ibv).cast()) };

        assert_eq!(list.index_buffer, mtl_buf);
        assert_eq!(list.index_buffer_va, va);
        assert_eq!(list.index_buffer_size, 256);
        assert_eq!(
            list.ib_cache_generation,
            dev.residency.va_generation.load(Ordering::Acquire)
        );

        // Inject a marker handle to prove same-generation calls hit the fast path, while a
        // generation bump clears `ib_cache` and forces a fresh VA lookup.
        list.index_buffer = 0xAAAA_0001;
        list.ib_cache[0] = (va, va + 256, 0xAAAA_0001);
        // SAFETY: `list_ptr` and `&ibv` point to valid test objects.
        unsafe { ia_set_index_buffer(list_ptr, (&raw const ibv).cast()) };
        assert_eq!(list.index_buffer, 0xAAAA_0001);

        residency::unregister(dev, &buf, va);
        // SAFETY: `list_ptr` and `&ibv` point to valid test objects.
        unsafe { ia_set_index_buffer(list_ptr, (&raw const ibv).cast()) };
        assert_eq!(list.index_buffer, 0);
        assert_eq!(list.index_buffer_va, 0);
        assert_eq!(list.ib_cache, [(0, 0, 0); 4]);

        // Re-registering restores lookup and bumps generation again.
        dev.residency
            .va_map
            .write()
            .expect("unpoisoned va_map lock")
            .insert(va, vec![Arc::clone(&buf)]);
        dev.residency.va_generation.fetch_add(1, Ordering::Release);
        // SAFETY: `list_ptr` and `&ibv` point to valid test objects.
        unsafe { ia_set_index_buffer(list_ptr, (&raw const ibv).cast()) };
        assert_eq!(list.index_buffer, mtl_buf);
        assert_eq!(list.index_buffer_va, va);
        assert_eq!(list.index_buffer_size, 256);
    }

    list.device = ptr::null_mut();
    drop(list);
    // SAFETY: `dev_ptr` was created via `Box::into_raw(Device::new_dummy())`.
    drop(unsafe { Box::from_raw(dev_ptr) });
}

#[test]
fn om_set_render_targets_readonly_dsv_sets_dont_care_and_skips_write() {
    let mut list = List::new_closed(ptr::null_mut());
    // WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 = 260 (both depth + stencil planes).
    let dsv_tex = make_test_texture(260, 1);
    let view = TextureViewDescriptor {
        format: 260,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    let tex_ptr = Arc::as_ptr(&dsv_tex).cast::<c_void>();

    let heap = make_render_target_heap(vec![
        // Index 0: both depth and stencil read-only (`flags = 3`).
        RenderTargetDesc {
            texture: Some(Arc::clone(&dsv_tex)),
            view,
            width: 64,
            height: 64,
            depth_plane: 0,
            render_target_array_length: 1,
            flags: 3,
        },
        // Index 1: depth read-only, stencil writable (`flags = 1`).
        RenderTargetDesc {
            texture: Some(Arc::clone(&dsv_tex)),
            view,
            width: 64,
            height: 64,
            depth_plane: 0,
            render_target_array_length: 1,
            flags: 1,
        },
    ]);

    let dsv_ro_both: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 0);
    let dsv_ro_depth: D3d12CpuDescriptorHandle =
        EmbeddedDescriptorHandle::pack(&raw const *heap, 1);
    let list_ptr: *mut c_void = (&raw mut *list).cast();

    // SAFETY: `list_ptr` and `&dsv_ro_both` are valid test pointers.
    unsafe { om_set_render_targets(list_ptr, 0, ptr::null(), 0, (&raw const dsv_ro_both).cast()) };
    assert_eq!(list.render_pass.depth.load_action, WMT_LOAD_ACTION_LOAD);
    assert_eq!(
        list.render_pass.depth.store_action,
        WMT_STORE_ACTION_DONT_CARE
    );
    assert_eq!(
        list.render_pass.stencil.store_action,
        WMT_STORE_ACTION_DONT_CARE
    );
    assert_eq!(list.render_pass.dsv_readonly_flags, 3);
    let writes = &list.render_pass.base.writes[..usize::from(list.render_pass.base.write_count)];
    assert!(!writes.contains(&tex_ptr));

    // SAFETY: `list_ptr` and `&dsv_ro_depth` are valid test pointers.
    unsafe {
        om_set_render_targets(
            list_ptr,
            0,
            ptr::null(),
            0,
            (&raw const dsv_ro_depth).cast(),
        );
    }
    assert_eq!(
        list.render_pass.depth.store_action,
        WMT_STORE_ACTION_DONT_CARE
    );
    assert_eq!(
        list.render_pass.stencil.store_action,
        WMT_STORE_ACTION_STORE
    );
    assert_eq!(list.render_pass.dsv_readonly_flags, 1);
    let writes = &list.render_pass.base.writes[..usize::from(list.render_pass.base.write_count)];
    assert!(writes.contains(&tex_ptr));
}

#[test]
fn list8_through_10_qi_stencil_and_dynamic_depth_bias() {
    let dev_ptr = Box::into_raw(Device::new_dummy());
    // SAFETY: `dev_ptr` is live; `List::drop` releases this reference.
    unsafe { add_ref_com(dev_ptr) };
    let mut list = List::new_closed(dev_ptr);
    let list_ptr: *mut c_void = (&raw mut *list).cast();
    // SAFETY: `list_ptr` is live; gives `list` initial refcount 1 so QI/Release stays > 0.
    unsafe { add_ref_com(list_ptr.cast::<List>()) };

    for iid in [
        IID_ID3D12_GRAPHICS_COMMAND_LIST8,
        IID_ID3D12_GRAPHICS_COMMAND_LIST9,
        IID_ID3D12_GRAPHICS_COMMAND_LIST10,
    ] {
        let mut out: *mut c_void = ptr::null_mut();
        // SAFETY: `list_ptr`, `iid`, and `out` are valid pointers.
        let hr = unsafe { query_interface(list_ptr, (&raw const iid).cast(), &raw mut out) };
        assert_eq!(hr, S_OK);
        assert_eq!(out, list_ptr);
        // SAFETY: balancing the reference added by `query_interface`.
        assert_eq!(unsafe { release_com(list_ptr.cast::<List>()) }, 1);
    }

    // SAFETY: `list_ptr` is a live `List`.
    unsafe { om_set_front_and_back_stencil_ref(list_ptr, 0x1ab, 0x2cd) };
    assert_eq!(list.stencil_ref, 0xab);
    assert!(list.dirty_state.contains(DirtyFlags::STENCIL_REF));

    // SAFETY: `list_ptr` is a live `List`.
    unsafe { (LIST_VTBL.om_set_front_and_back_stencil_ref)(list_ptr, 0x34, 0x34) };
    assert_eq!(list.stencil_ref, 0x34);

    // SAFETY: `LIST_VTBL` is larger than `ID3D12PipelineStateVtbl`; slots are never invoked.
    let pso_vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12PipelineStateVtbl>() };
    // SAFETY: `crate::tests` provides no-op `Obj::retain`/`release` stubs.
    let pso_obj = unsafe { Obj::adopt(1) };
    let mut pso = Graphics::new_dummy(pso_vtbl, pso_obj);
    let pso_ptr: *mut c_void = (&raw mut *pso).cast();

    // SAFETY: `list_ptr` and `pso_ptr` are valid pointers.
    unsafe { set_pipeline_state(list_ptr, pso_ptr) };
    list.dirty_state
        .remove(DirtyFlags::PSO | DirtyFlags::RASTERIZER);

    // SAFETY: `list_ptr` is a live `List`.
    unsafe { rs_set_depth_bias(list_ptr, 2.0, 0.5, 1.5) };
    assert_eq!(list.dynamic_depth_bias, Some((2.0, 0.5, 1.5)));
    assert!(list.dirty_state.contains(DirtyFlags::RASTERIZER));

    // Rebinding the same graphics PSO clears dynamic depth bias and dirties RASTERIZER.
    list.dirty_state.remove(DirtyFlags::RASTERIZER);
    // SAFETY: `list_ptr` and `pso_ptr` are valid pointers.
    unsafe { set_pipeline_state(list_ptr, pso_ptr) };
    assert_eq!(list.dynamic_depth_bias, None);
    assert!(list.dirty_state.contains(DirtyFlags::RASTERIZER));

    // `RSSetDepthBias` vtable dispatch + `reset_state` clears dynamic depth bias.
    // SAFETY: `list_ptr` is a live `List`.
    unsafe { (LIST_VTBL.rs_set_depth_bias)(list_ptr, 3.0, 0.25, 0.75) };
    assert_eq!(list.dynamic_depth_bias, Some((3.0, 0.25, 0.75)));
    reset_state(&mut list, core::ptr::null_mut());
    assert_eq!(list.dynamic_depth_bias, None);

    // SAFETY: `list_ptr` is valid; disabled strip cut value and graph stubs are safe no-ops.
    unsafe { (LIST_VTBL.ia_set_index_buffer_strip_cut_value)(list_ptr, 0) };
    // SAFETY: `list_ptr` is valid; null desc is a safe no-op.
    unsafe { (LIST_VTBL.set_program)(list_ptr, ptr::null()) };
    // SAFETY: `list_ptr` is valid; null desc is a safe no-op.
    unsafe { (LIST_VTBL.dispatch_graph)(list_ptr, ptr::null()) };

    drop(list);
    drop(pso);
    // SAFETY: `dev_ptr` was allocated by `Box::into_raw` with refcount 1.
    unsafe { release_com(dev_ptr) };
}

#[test]
fn ext_and_ext1_qi_return_companion_pointer() {
    let dev_ptr = Box::into_raw(Device::new_dummy());
    // SAFETY: `dev_ptr` is live; `List::drop` releases this reference.
    unsafe { add_ref_com(dev_ptr) };
    let mut list = List::new_closed(dev_ptr);
    let list_ptr: *mut c_void = (&raw mut *list).cast();
    // SAFETY: `list_ptr` is live; gives `list` initial refcount 1 so QI/Release stays > 0.
    unsafe { add_ref_com(list_ptr.cast::<List>()) };

    let mut ext0: *mut c_void = ptr::null_mut();
    let mut ext1: *mut c_void = ptr::null_mut();
    let iid_ext = IID_D3D12_COMMAND_LIST_EXT;
    let iid_ext1 = IID_IMTL_D3D12_COMMAND_LIST_EXT1;
    let iid_unk = IID_IUNKNOWN;

    // SAFETY: `list_ptr`, `iid_ext`, and `ext0` are valid pointers.
    let hr = unsafe { query_interface(list_ptr, (&raw const iid_ext).cast(), &raw mut ext0) };
    assert_eq!(hr, S_OK);
    assert_eq!(ext0.addr(), list_ptr.addr() + List::EXT_OFFSET);

    // SAFETY: `list_ptr`, `iid_ext1`, and `ext1` are valid pointers.
    let hr = unsafe { query_interface(list_ptr, (&raw const iid_ext1).cast(), &raw mut ext1) };
    assert_eq!(hr, S_OK);
    assert_eq!(ext1, ext0);

    let mut unk: *mut c_void = ptr::null_mut();
    // SAFETY: `ext1` is the companion pointer at `List::EXT_OFFSET`.
    let hr = unsafe { ext_query_interface(ext1, (&raw const iid_unk).cast(), &raw mut unk) };
    assert_eq!(hr, S_OK);
    assert_eq!(unk, list_ptr);

    // SAFETY: balance `unk`, `ext1`, and `ext0` references while keeping `list`'s initial ref.
    assert_eq!(unsafe { release_com(list_ptr.cast::<List>()) }, 3);
    // SAFETY: `ext1` is at `List::EXT_OFFSET`.
    assert_eq!(unsafe { ext_release(ext1) }, 2);
    // SAFETY: `ext0` is at `List::EXT_OFFSET`.
    assert_eq!(unsafe { ext_release(ext0) }, 1);

    drop(list);
    // SAFETY: `dev_ptr` was allocated by `Box::into_raw` with refcount 1.
    unsafe { release_com(dev_ptr) };
}
