use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R8_TYPELESS, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R16_TYPELESS, DXGI_FORMAT_R16_UNORM,
    DXGI_FORMAT_R32_FLOAT,
};

use super::{
    consume_reset, reactive_format_compatible, set_frame_gen_ui, take_optional_denoised,
    temporal_denoised_upscale,
};
use crate::{
    abi::ext::{D3D12DenoisedUpscaleDesc, D3D12FrameGenUiDesc},
    com::{E_INVALIDARG, S_OK, add_ref_com, release_com},
    device::Device,
    list::List,
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

    // Null `desc` on `set_frame_gen_ui` returns `E_INVALIDARG`; null `ui_texture` succeeds (`S_OK`).
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
    // SAFETY: `ext_this` and `ui_desc` are valid pointers.
    let hr = unsafe { set_frame_gen_ui(ext_this, &raw const ui_desc) };
    assert_eq!(hr, S_OK);
    // Clear UI state on shared framegen slot after test.
    let zero_ui = D3D12FrameGenUiDesc {
        ui_composited: 0,
        content_width: 0,
        content_height: 0,
        depth_offset: [0; 2],
        motion_offset: [0; 2],
        output_offset: [0; 2],
        ..ui_desc
    };
    // SAFETY: `ext_this` and `zero_ui` are valid pointers.
    let hr = unsafe { set_frame_gen_ui(ext_this, &raw const zero_ui) };
    assert_eq!(hr, S_OK);

    drop(list);
    // SAFETY: `dev_ptr` was allocated by `Box::into_raw` with refcount 1.
    unsafe { release_com(dev_ptr) };
}
