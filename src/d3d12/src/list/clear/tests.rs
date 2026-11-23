//! Unit tests for `BeginRenderPass`, `EndRenderPass`, `DiscardResource`, and inline resolve.

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, align_of, size_of},
    ptr,
    sync::atomic::AtomicBool,
};
use std::sync::{Arc, Mutex};

use dxmt_core::resource::{Texture, TextureAllocationFlags, TextureViewDescriptor};
use dxmt_util::{
    com::{ComObject, S_OK},
    dxgi_consts::DXGI_FORMAT_R8G8B8A8_UNORM,
    private_data::PrivateData,
};
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_LOAD_ACTION_DONT_CARE, WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_DONT_CARE,
    WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D, WMTTextureInfo,
};

use super::{begin_render_pass, clear_render_target_view, discard_resource, end_render_pass};
use crate::{
    abi::{
        list_ops::{
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_PRESERVE,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_PRESERVE,
            D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_DEPTH, D3D12_RENDER_PASS_FLAG_RESUMING_PASS,
            D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS, DiscardRegion, RenderPassBeginningAccess,
            RenderPassBeginningAccessBody, RenderPassDepthStencilDesc, RenderPassEndingAccess,
            RenderPassEndingAccessBody, RenderPassRenderTargetDesc,
        },
        list_state::D3D12_COMMAND_LIST_TYPE_DIRECT,
        objects::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        pipeline::SampleDesc,
        resource::{D3D12_RESOURCE_DIMENSION_TEXTURE2D, HeapProperties, ResourceDesc},
        vtbl::{D3d12CpuDescriptorHandle, ID3D12DescriptorHeapVtbl, ID3D12Resource2Vtbl},
    },
    allocator::{Allocator, cast_allocator, create_command_allocator},
    com::{IID_ID3D12_COMMAND_ALLOCATOR, IID_ID3D12_GRAPHICS_COMMAND_LIST, release_com},
    descriptor_heap::{EmbeddedDescriptorHandle, Heap, RenderTargetDesc},
    encoder::{EncoderData, EncoderType, RenderEncoderData, TextureViewRef},
    list::{
        LIST_VTBL, List,
        state::{close, create_command_list},
    },
    resource::Resource,
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

fn make_test_texture_resource(
    tex: Arc<Texture>,
    array_size: u16,
    mip_levels: u16,
) -> Box<Resource> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12Resource2Vtbl` and consists of non-null
    // function pointers; tests never dispatch through `Resource::vtbl`.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12Resource2Vtbl>() };
    Box::new(Resource {
        vtbl,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        texture: Some(tex),
        buffer: None,
        placed_heap: ptr::null_mut(),
        desc: ResourceDesc {
            dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            alignment: 0,
            width: 64,
            height: 64,
            depth_or_array_size: array_size,
            mip_levels,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: SampleDesc {
                count: 1,
                quality: 0,
            },
            layout: 0,
            flags: 0,
        },
        heap_props: HeapProperties {
            r#type: 1,
            cpu_page_property: 0,
            memory_pool_preference: 0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
    })
}

fn make_rt_heap(entries: Vec<RenderTargetDesc>) -> Box<Heap> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12DescriptorHeapVtbl` and consists of non-null
    // function pointers; tests never dispatch through heap vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12DescriptorHeapVtbl>() };
    Box::new(Heap {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        heap_type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
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

fn new_recording_list() -> (*mut c_void, *mut Allocator, *mut c_void, *mut List) {
    let alloc_iid = IID_ID3D12_COMMAND_ALLOCATOR;
    let mut raw_alloc: *mut c_void = ptr::null_mut();
    // SAFETY: `alloc_iid` and `raw_alloc` are valid out-parameters for test creation.
    let hr = unsafe {
        create_command_allocator(
            ptr::null_mut(),
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            (&raw const alloc_iid).cast(),
            &raw mut raw_alloc,
        )
    };
    assert_eq!(hr, S_OK);
    // SAFETY: `raw_alloc` was created by `create_command_allocator`.
    let alloc = unsafe { cast_allocator(raw_alloc) }.expect("valid Allocator pointer");

    let list_iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();
    // SAFETY: `raw_alloc`, `list_iid`, and `raw_list` are valid test pointers.
    let hr = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            raw_alloc,
            ptr::null_mut(),
            (&raw const list_iid).cast(),
            &raw mut raw_list,
        )
    };
    assert_eq!(hr, S_OK);
    let list_ptr = raw_list.cast::<List>();
    (raw_alloc, alloc, raw_list, list_ptr)
}

fn begin_access(access_type: u32) -> RenderPassBeginningAccess {
    RenderPassBeginningAccess {
        r#type: access_type,
        access: RenderPassBeginningAccessBody {
            // SAFETY: `RenderPassBeginningAccessBody` is a C union; zeroed storage is valid
            // when `r#type != CLEAR`.
            clear: ManuallyDrop::new(unsafe { core::mem::zeroed() }),
        },
    }
}

fn end_access(access_type: u32) -> RenderPassEndingAccess {
    RenderPassEndingAccess {
        r#type: access_type,
        access: RenderPassEndingAccessBody {
            // SAFETY: `RenderPassEndingAccessBody` is a C union; zeroed storage is valid when
            // `r#type != RESOLVE`.
            resolve: ManuallyDrop::new(unsafe { core::mem::zeroed() }),
        },
    }
}

#[test]
fn begin_and_end_render_pass_handle_discard_and_ro_dsv() {
    let (raw_alloc, alloc_ptr, raw_list, list_ptr) = new_recording_list();
    let rtv_tex = make_test_texture(70, 1);
    let dsv_tex = make_test_texture(260, 1);
    let dsv_ptr = Arc::as_ptr(&dsv_tex).cast::<c_void>();

    let rtv_key = TextureViewDescriptor {
        format: 70,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    let dsv_key = TextureViewDescriptor {
        format: 260,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();

    let heap = make_rt_heap(vec![
        RenderTargetDesc {
            texture: Some(Arc::clone(&rtv_tex)),
            view: rtv_key,
            width: 64,
            height: 64,
            depth_plane: 0,
            render_target_array_length: 1,
            flags: 0,
        },
        RenderTargetDesc {
            texture: Some(Arc::clone(&dsv_tex)),
            view: dsv_key,
            width: 64,
            height: 64,
            depth_plane: 0,
            render_target_array_length: 1,
            flags: 0,
        },
    ]);
    let rtv_handle: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 0);
    let dsv_handle: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 1);

    let rt_desc = RenderPassRenderTargetDesc {
        cpu_descriptor: rtv_handle,
        beginning_access: begin_access(D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD),
        ending_access: end_access(D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD),
    };
    let ds_desc = RenderPassDepthStencilDesc {
        cpu_descriptor: dsv_handle,
        depth_beginning_access: begin_access(D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD),
        stencil_beginning_access: begin_access(D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_PRESERVE),
        depth_ending_access: end_access(D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD),
        stencil_ending_access: end_access(D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_PRESERVE),
    };

    // Pass 1: RESUMING | SUSPENDING | BIND_READ_ONLY_DEPTH suppresses both beginning and ending
    // DISCARD while marking depth read-only.
    // SAFETY: `raw_list`, `&rt_desc`, and `&ds_desc` are valid test pointers.
    unsafe {
        begin_render_pass(
            raw_list,
            1,
            (&raw const rt_desc).cast(),
            (&raw const ds_desc).cast(),
            D3D12_RENDER_PASS_FLAG_RESUMING_PASS
                | D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS
                | D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_DEPTH,
        );
    }
    // SAFETY: `list_ptr` is live until `release_com`.
    let list = unsafe { &*list_ptr };
    assert!(list.render_pass_state.active);
    assert_eq!(
        list.render_pass_state.color_load_override[0],
        WMT_LOAD_ACTION_LOAD
    );
    assert_eq!(
        list.render_pass_state.depth_load_override,
        WMT_LOAD_ACTION_LOAD
    );
    assert_eq!(list.render_pass_state.color_ending_discard, 0);
    assert_eq!(list.render_pass_state.dsv_ending_discard, 0);
    assert_eq!(
        list.render_pass.depth.store_action,
        WMT_STORE_ACTION_DONT_CARE
    );
    assert_eq!(list.render_pass.dsv_readonly_flags & 1, 1);
    let ro_writes = &list.render_pass.base.writes[..usize::from(list.render_pass.base.write_count)];
    assert!(ro_writes.contains(&dsv_ptr));

    // SAFETY: `raw_list` is valid for ending the active render pass.
    unsafe { end_render_pass(raw_list) };

    // Pass 2: normal flags enable beginning and ending DISCARD; simulate an active draw pass.
    // SAFETY: `raw_list`, `&rt_desc`, and `&ds_desc` are valid test pointers.
    unsafe {
        begin_render_pass(
            raw_list,
            1,
            (&raw const rt_desc).cast(),
            (&raw const ds_desc).cast(),
            0,
        );
    }
    // SAFETY: `list_ptr` and `alloc_ptr` are valid until closed/released.
    let list_mut = unsafe { &mut *list_ptr };
    // SAFETY: `alloc_ptr` is valid for the duration of recording.
    let alloc = unsafe { &mut *alloc_ptr };
    assert_eq!(
        list_mut.render_pass_state.color_load_override[0],
        WMT_LOAD_ACTION_DONT_CARE
    );
    assert_eq!(
        list_mut.render_pass_state.depth_load_override,
        WMT_LOAD_ACTION_DONT_CARE
    );
    assert_eq!(
        list_mut.render_pass_state.stencil_load_override,
        WMT_LOAD_ACTION_LOAD
    );
    assert_eq!(list_mut.render_pass_state.color_ending_discard, 1);
    assert_eq!(list_mut.render_pass_state.dsv_ending_discard, 1);

    let r_ptr = alloc
        .allocate_cpu_heap(
            size_of::<RenderEncoderData>(),
            align_of::<RenderEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<RenderEncoderData>();
    assert!(!r_ptr.is_null());
    let mut r_pass = List::empty_render_pass();
    r_pass.base = EncoderData::new(EncoderType::Render, 0);
    r_pass.colors[0].attachment = TextureViewRef::from_texture(&rtv_tex, rtv_key);
    r_pass.colors[0].store_action = WMT_STORE_ACTION_STORE;
    r_pass.depth.attachment = TextureViewRef::from_texture(&dsv_tex, dsv_key);
    r_pass.depth.store_action = WMT_STORE_ACTION_STORE;
    r_pass.stencil.attachment = TextureViewRef::from_texture(&dsv_tex, dsv_key);
    r_pass.stencil.store_action = WMT_STORE_ACTION_STORE;
    // SAFETY: `r_ptr` points to aligned writable CPU heap memory.
    unsafe { ptr::write(r_ptr, r_pass) };
    alloc.encoder_current = r_ptr.cast::<EncoderData>();
    list_mut.inside_render_pass = true;

    // SAFETY: `raw_list` is valid for ending the active render pass.
    unsafe { end_render_pass(raw_list) };
    // SAFETY: `r_ptr` remains valid in `alloc`'s CPU heap.
    let r_after = unsafe { &*r_ptr };
    assert_eq!(r_after.colors[0].store_action, WMT_STORE_ACTION_DONT_CARE);
    assert_eq!(r_after.depth.store_action, WMT_STORE_ACTION_DONT_CARE);
    assert_eq!(r_after.stencil.store_action, WMT_STORE_ACTION_STORE);
    // SAFETY: `list_ptr` remains valid.
    let list_after = unsafe { &*list_ptr };
    assert!(!list_after.render_pass_state.active);
    assert!(!list_after.inside_render_pass);
    // SAFETY: `alloc_ptr` remains valid.
    assert!(unsafe { &*alloc_ptr }.encoder_current.is_null());

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` and `raw_alloc` hold one refcount each.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `raw_alloc` holds one refcount.
    unsafe { release_com::<Allocator>(raw_alloc.cast()) };
}

#[test]
fn discard_resource_records_and_purges_on_clear() {
    let (raw_alloc, alloc_ptr, raw_list, list_ptr) = new_recording_list();
    let tex = make_test_texture(70, 1);
    let tex_ptr = Arc::as_ptr(&tex).cast::<c_void>();
    let mut res = make_test_texture_resource(Arc::clone(&tex), 2, 2);

    let key = TextureViewDescriptor {
        format: 70,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    let heap = make_rt_heap(vec![RenderTargetDesc {
        texture: Some(Arc::clone(&tex)),
        view: key,
        width: 64,
        height: 64,
        depth_plane: 0,
        render_target_array_length: 1,
        flags: 0,
    }]);
    let rtv_handle: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 0);

    // Partial rect region (`num_rects != 0`) must be ignored.
    let bad_region = DiscardRegion {
        num_rects: 1,
        rects: ptr::null(),
        first_subresource: 0,
        num_subresources: 1,
    };
    let res_ptr: *mut c_void = (&raw mut *res).cast();
    // SAFETY: `raw_list`, `res_ptr`, and `&bad_region` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, (&raw const bad_region).cast()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());

    // Subresource range (mip 1 slice 1 = index 3 for mip_levels=2) is expanded cleanly.
    let good_region = DiscardRegion {
        num_rects: 0,
        rects: ptr::null(),
        first_subresource: 3,
        num_subresources: 1,
    };
    // SAFETY: `raw_list`, `res_ptr`, and `&good_region` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, (&raw const good_region).cast()) };
    // SAFETY: `list_ptr` and `alloc_ptr` are live for the test.
    let list = unsafe { &*list_ptr };
    assert_eq!(list.discarded_resources.len(), 1);
    assert_eq!(list.discarded_resources[0].tex_ptr, tex_ptr);
    assert_eq!(list.discarded_resources[0].mip, 1);
    assert_eq!(list.discarded_resources[0].slice, 1);
    assert!(!list.discarded_resources[0].all);
    // SAFETY: `alloc_ptr` is valid while recording.
    assert!(unsafe { &*alloc_ptr }.discard_tracking);

    // Clearing the resource purges its pending discarded entries so the clear is not lost.
    let clear_color = [0.25_f32, 0.5, 0.75, 1.0];
    // SAFETY: `raw_list` and `clear_color.as_ptr()` are valid test pointers.
    unsafe { clear_render_target_view(raw_list, rtv_handle, clear_color.as_ptr(), 0, ptr::null()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` and `raw_alloc` hold one refcount each.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `raw_alloc` holds one refcount.
    unsafe { release_com::<Allocator>(raw_alloc.cast()) };
}

#[test]
fn discard_resource_region_respects_subresource_bounds_and_open_pass() {
    let (raw_alloc, alloc_ptr, raw_list, list_ptr) = new_recording_list();
    let tex = make_test_texture(70, 1);
    let tex_ptr = Arc::as_ptr(&tex).cast::<c_void>();
    // 1 mip x 128 slices = 128 total subresources.
    let mut res = make_test_texture_resource(Arc::clone(&tex), 128, 1);
    let res_ptr: *mut c_void = (&raw mut *res).cast();

    // Partial region (> 64 subresources, < total) must NOT widen to `all: true`.
    let partial_large = DiscardRegion {
        num_rects: 0,
        rects: ptr::null(),
        first_subresource: 0,
        num_subresources: 65,
    };
    // SAFETY: `raw_list`, `res_ptr`, and `&partial_large` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, (&raw const partial_large).cast()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());
    // SAFETY: `alloc_ptr` is valid while recording.
    assert!(!unsafe { &*alloc_ptr }.discard_tracking);

    // Non-zero `first_subresource` (> 64 subresources) also must NOT widen to `all: true`.
    let offset_large = DiscardRegion {
        num_rects: 0,
        rects: ptr::null(),
        first_subresource: 1,
        num_subresources: 127,
    };
    // SAFETY: `raw_list`, `res_ptr`, and `&offset_large` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, (&raw const offset_large).cast()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());

    // Planar format (D24_UNORM_S8_UINT = 45, 2 planes x 64 slices = 128 total) counts planes too.
    let mut dsv_res = make_test_texture_resource(Arc::clone(&tex), 64, 1);
    dsv_res.desc.format = 45;
    let dsv_res_ptr: *mut c_void = (&raw mut *dsv_res).cast();
    // SAFETY: `raw_list`, `dsv_res_ptr`, and `&partial_large` are valid test pointers.
    unsafe { discard_resource(raw_list, dsv_res_ptr, (&raw const partial_large).cast()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());

    // Full range (`first == 0 && count >= total`) sets `all: true`.
    let full_large = DiscardRegion {
        num_rects: 0,
        rects: ptr::null(),
        first_subresource: 0,
        num_subresources: 128,
    };
    // SAFETY: `raw_list`, `res_ptr`, and `&full_large` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, (&raw const full_large).cast()) };
    // SAFETY: `list_ptr` is live for the test.
    let entries = &unsafe { &*list_ptr }.discarded_resources;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tex_ptr, tex_ptr);
    assert!(entries[0].all);

    // Discarding a resource currently bound in an active open render pass must skip recording.
    // SAFETY: `list_ptr` is mutable and exclusive to this test.
    unsafe { &mut *list_ptr }.discarded_resources.clear();
    let key = TextureViewDescriptor {
        format: 70,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    // SAFETY: `alloc_ptr` is valid while recording.
    let alloc = unsafe { &mut *alloc_ptr };
    let r_ptr = alloc
        .allocate_cpu_heap(
            size_of::<RenderEncoderData>(),
            align_of::<RenderEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<RenderEncoderData>();
    assert!(!r_ptr.is_null());
    let mut r_pass = List::empty_render_pass();
    r_pass.base = EncoderData::new(EncoderType::Render, 0);
    r_pass.colors[0].attachment = TextureViewRef::from_texture(&tex, key);
    // SAFETY: `r_ptr` points to aligned writable CPU heap memory.
    unsafe { ptr::write(r_ptr, r_pass) };
    alloc.encoder_current = r_ptr.cast::<EncoderData>();

    // SAFETY: `raw_list` and `res_ptr` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, ptr::null()) };
    // SAFETY: `list_ptr` is live for the test.
    assert!(unsafe { &*list_ptr }.discarded_resources.is_empty());

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` and `raw_alloc` hold one refcount each.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `raw_alloc` holds one refcount.
    unsafe { release_com::<Allocator>(raw_alloc.cast()) };
}
