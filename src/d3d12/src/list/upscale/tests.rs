use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::Arc;

use dxmt_core::resource::{Texture, TextureAllocationFlags};
use dxmt_util::{
    com::ComObject,
    dxgi_consts::{
        DXGI_FORMAT_R8_TYPELESS, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R16_TYPELESS, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R32_FLOAT,
    },
    private_data::PrivateData,
};
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_PIXEL_FORMAT_BC1_RGBA, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_R8_UINT, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_SHADER_READ, WMTTextureInfo,
};

use super::{
    DLSSG_CAMERA_GUID, consume_reset, reactive_format_compatible, set_frame_gen_ui,
    take_optional_denoised, temporal_denoised_upscale,
};
use crate::{
    abi::{
        ext::{D3D12DenoisedUpscaleDesc, D3D12FrameGenUiDesc},
        list_state::D3D12_COMMAND_LIST_TYPE_DIRECT,
        pipeline::SampleDesc,
        resource::{D3D12_RESOURCE_DIMENSION_TEXTURE2D, HeapProperties, ResourceDesc},
        vtbl::ID3D12Resource2Vtbl,
    },
    allocator::{Allocator, cast_allocator, create_command_allocator},
    com::{
        E_INVALIDARG, IID_ID3D12_COMMAND_ALLOCATOR, IID_ID3D12_GRAPHICS_COMMAND_LIST, S_OK,
        add_ref_com, release_com,
    },
    device::Device,
    encoder::{EncoderType, FrameGenUiEncoderData},
    list::{LIST_VTBL, List, state::create_command_list},
    resource::Resource,
};

#[test]
fn explicit_reset_short_circuits_fresh_flag() {
    let fresh = AtomicBool::new(true);
    assert_eq!(consume_reset(1, &fresh), 1);
    assert!(fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 1);
    assert!(!fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 0);
}

#[test]
fn reactive_format_compatible_handles_typeless_and_concrete() {
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R8_UNORM,
        DXGI_FORMAT_R8_UNORM
    ));
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R8_UNORM
    ));
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R16_TYPELESS,
        DXGI_FORMAT_R16_UNORM
    ));
    assert!(!reactive_format_compatible(
        DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R32_FLOAT
    ));
}

#[test]
fn denoised_upscale_and_frame_gen_ui_validation() {
    let dev_ptr = Box::into_raw(Device::new_dummy());
    // SAFETY: `dev_ptr` is live; `List::drop` releases this reference.
    unsafe { add_ref_com(dev_ptr) };
    let mut list = List::new_closed(dev_ptr);
    let list_ptr: *mut c_void = (&raw mut *list).cast();
    // SAFETY: `List::EXT_OFFSET` is within `List`.
    let ext_this = unsafe { list_ptr.byte_add(List::EXT_OFFSET) };

    let dummy_desc = D3D12DenoisedUpscaleDesc {
        color: ptr::null_mut(),
        output: ptr::null_mut(),
        depth: ptr::null_mut(),
        motion_vector: ptr::null_mut(),
        diffuse_albedo: ptr::null_mut(),
        specular_albedo: ptr::null_mut(),
        normal: ptr::null_mut(),
        roughness: ptr::null_mut(),
        specular_hit_distance: ptr::null_mut(),
        denoise_strength_mask: ptr::null_mut(),
        transparency_overlay: ptr::null_mut(),
        exposure: ptr::null_mut(),
        reactive_mask: ptr::null_mut(),
        input_content_width: 0,
        input_content_height: 0,
        jitter_offset: [0.0; 2],
        motion_vector_scale: [0.0; 2],
        pre_exposure: 1.0,
        reset: 0,
        depth_reversed: 0,
        has_matrices: 0,
        world_to_view_matrix: [0.0; 16],
        view_to_clip_matrix: [0.0; 16],
    };

    // SAFETY: null `scaler` and null `desc` are rejected with `E_INVALIDARG`.
    let hr = unsafe { temporal_denoised_upscale(ext_this, ptr::null_mut(), &raw const dummy_desc) };
    assert_eq!(hr, E_INVALIDARG);
    // SAFETY: null `desc` is rejected with `E_INVALIDARG`.
    let hr = unsafe { temporal_denoised_upscale(ext_this, ext_this, ptr::null()) };
    assert_eq!(hr, E_INVALIDARG);

    // Optional channel helper rejects null when enabled and non-null when disabled.
    assert!(matches!(
        take_optional_denoised(ptr::null_mut(), false, DXGI_FORMAT_R8_UNORM, 64, 64),
        Ok(None)
    ));
    let Err(hr) = take_optional_denoised(ptr::null_mut(), true, DXGI_FORMAT_R8_UNORM, 64, 64)
    else {
        panic!("expected error");
    };
    assert_eq!(hr, E_INVALIDARG);

    // Closed list (null allocator) returns `E_INVALIDARG` for `set_frame_gen_ui`.
    // SAFETY: null `desc` is rejected.
    let hr = unsafe { set_frame_gen_ui(ext_this, ptr::null()) };
    assert_eq!(hr, E_INVALIDARG);
    let ui_desc = D3D12FrameGenUiDesc {
        ui_texture: ptr::null_mut(),
        ui_composited: 1,
        content_width: 1280,
        content_height: 720,
        depth_offset: [1, 2],
        motion_offset: [3, 4],
        output_offset: [5, 6],
        has_matrices: 0,
        world_to_view_matrix: [0.0; 16],
        view_to_clip_matrix: [0.0; 16],
    };
    // SAFETY: closed list has `allocator == null`, so `set_frame_gen_ui` returns `E_INVALIDARG`.
    let hr = unsafe { set_frame_gen_ui(ext_this, &raw const ui_desc) };
    assert_eq!(hr, E_INVALIDARG);

    drop(list);
    // SAFETY: `dev_ptr` was allocated by `Box::into_raw` with refcount 1.
    unsafe { release_com(dev_ptr) };
}

fn make_ui_resource(pixel_format: u32, usage: u32, width: u32, height: u32) -> Box<Resource> {
    let bits = (WMT_TEXTURE_TYPE2D & 0xff) | (1 << 8) | (1 << 16) | ((usage & 0xff) << 24);
    let info = WMTTextureInfo {
        pixel_format,
        width,
        height,
        depth: 1,
        array_length: 1,
        bits,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    // SAFETY: `crate::tests` stubs retain/release for test `Obj` handles.
    let obj = unsafe { Obj::adopt(1) }.expect("non-zero fake handle");
    let tex = Arc::new(Texture::new(info, &obj));
    let alloc = tex.allocate(TextureAllocationFlags::SHARED);
    let _ = tex.rename(alloc);
    // SAFETY: `LIST_VTBL` is larger than `ID3D12Resource2Vtbl`; tests never call resource slots.
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
            width: u64::from(width),
            height,
            depth_or_array_size: 1,
            mip_levels: 1,
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

fn new_open_list() -> (*mut c_void, *mut Allocator, *mut c_void, *mut List) {
    let alloc_iid = IID_ID3D12_COMMAND_ALLOCATOR;
    let mut raw_alloc: *mut c_void = ptr::null_mut();
    // SAFETY: `alloc_iid` and `raw_alloc` are valid out-parameters.
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
    let alloc = unsafe { cast_allocator(raw_alloc) }.expect("valid Allocator");

    let list_iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();
    // SAFETY: `raw_alloc`, `list_iid`, and `raw_list` are valid pointers.
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
    (raw_alloc, alloc, raw_list, raw_list.cast::<List>())
}

#[test]
fn set_frame_gen_ui_records_encoder_and_validates_texture() {
    let (raw_alloc, alloc_ptr, raw_list, list_ptr) = new_open_list();
    // SAFETY: `List::EXT_OFFSET` is within `List`.
    let ext_this = unsafe { raw_list.byte_add(List::EXT_OFFSET) };

    // Unreadable, depth, BC, and integer UI textures are rejected with `E_INVALIDARG`.
    let unreadable = make_ui_resource(WMT_PIXEL_FORMAT_BGRA8_UNORM, 0, 1920, 1080);
    let depth_tex = make_ui_resource(
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        WMT_TEXTURE_USAGE_SHADER_READ,
        1920,
        1080,
    );
    let bc_tex = make_ui_resource(
        WMT_PIXEL_FORMAT_BC1_RGBA,
        WMT_TEXTURE_USAGE_SHADER_READ,
        1920,
        1080,
    );
    let int_tex = make_ui_resource(
        WMT_PIXEL_FORMAT_R8_UINT,
        WMT_TEXTURE_USAGE_SHADER_READ,
        1920,
        1080,
    );
    let mut desc = D3D12FrameGenUiDesc {
        ui_texture: ptr::null_mut(),
        ui_composited: 1,
        content_width: 1280,
        content_height: 720,
        depth_offset: [1, 2],
        motion_offset: [3, 4],
        output_offset: [5, 6],
        has_matrices: 0,
        world_to_view_matrix: [0.0; 16],
        view_to_clip_matrix: [0.0; 16],
    };
    for bad in [&unreadable, &depth_tex, &bc_tex, &int_tex] {
        desc.ui_texture = ptr::from_ref(&**bad).cast_mut().cast::<c_void>();
        // SAFETY: `ext_this` and `desc` are valid pointers.
        let hr = unsafe { set_frame_gen_ui(ext_this, &raw const desc) };
        assert_eq!(hr, E_INVALIDARG);
    }

    // Valid readable color texture + camera private data records `EncoderType::FrameGenUi`.
    let cam = [0.25f32, 5000.0, 65.0, 1.777_777_8];
    // SAFETY: `list_ptr` is a live `List`.
    let list_ref = unsafe { &*list_ptr };
    // SAFETY: `cam` is 16 readable bytes.
    let hr = unsafe {
        list_ref
            .private_data
            .set_data(DLSSG_CAMERA_GUID, 16, cam.as_ptr().cast::<c_void>())
    };
    assert_eq!(hr, S_OK);

    let good = make_ui_resource(
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_TEXTURE_USAGE_SHADER_READ,
        1920,
        1080,
    );
    desc.ui_texture = ptr::from_ref(&*good).cast_mut().cast::<c_void>();
    // SAFETY: `ext_this` and `desc` are valid pointers.
    let hr = unsafe { set_frame_gen_ui(ext_this, &raw const desc) };
    assert_eq!(hr, S_OK);

    // SAFETY: `alloc_ptr` is live; `set_frame_gen_ui` invalidated the pass into `encoder_last`.
    let last = unsafe { (*alloc_ptr).encoder_last };
    assert!(!last.is_null());
    // SAFETY: `last` is a live `EncoderData` header.
    assert_eq!(unsafe { (*last).kind }, EncoderType::FrameGenUi);
    // SAFETY: `kind == FrameGenUi` guarantees `FrameGenUiEncoderData`.
    let ui_enc = unsafe { &*last.cast::<FrameGenUiEncoderData>() };
    assert!(ui_enc.ui_texture.is_some());
    assert_eq!(ui_enc.ui_format, WMT_PIXEL_FORMAT_BGRA8_UNORM);
    assert_eq!(ui_enc.ui_width, 1920);
    assert_eq!(ui_enc.ui_height, 1080);
    assert!(ui_enc.ui.ui_composited);
    assert_eq!(ui_enc.ui.content_width, 1280);
    assert_eq!(ui_enc.ui.content_height, 720);
    let recorded_cam = ui_enc.camera.as_ref().expect("camera recorded");
    assert_eq!(recorded_cam.near_plane, 0.25);
    assert_eq!(recorded_cam.far_plane, 5000.0);

    // SAFETY: `raw_list` holds initial COM refcount 1.
    unsafe { release_com(raw_list.cast::<List>()) };
    // SAFETY: `raw_alloc` holds initial COM refcount 1.
    unsafe { release_com(raw_alloc.cast::<Allocator>()) };
}
