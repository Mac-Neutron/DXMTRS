//! The COM interfaces nvngx calls, and the structures they take.
//!
//! `IUnknown`, the D3D11 and D3D12 slots nvngx uses, and DXMT's private extensions
//! (`d3d11_interfaces.hpp`, `d3d12_interfaces.hpp`); a slot nvngx never calls is an opaque
//! pointer. Layouts are the C++ headers', checked by the asserts below.

use core::ffi::c_void;

pub use dxmt_util::com::{Guid, Hresult, IUnknownVtbl, query, release, vtbl};

/// `ID3D12Device`'s IID.
pub static IID_ID3D12_DEVICE: Guid = Guid::new(
    0x1898_19f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);
/// DXMT's `IMTLD3D12DeviceExt`.
pub static IID_DEVICE_EXT_12: Guid = Guid::new(
    0xa8c6_4ba7,
    0x6d54,
    0x4833,
    [0x8c, 0xc6, 0x26, 0x05, 0xc5, 0xb4, 0x54, 0xbe],
);
/// DXMT's `IMTLD3D12CommandListExt`.
pub static IID_COMMAND_LIST_EXT_12: Guid = Guid::new(
    0x5690_e8a7,
    0x51a1,
    0x4b37,
    [0x8b, 0xbd, 0x73, 0x0d, 0x0f, 0xdb, 0x2c, 0xec],
);
/// DXMT's `IMTLD3D11ContextExt`.
pub static IID_CONTEXT_EXT_11: Guid = Guid::new(
    0x43ac_e3ce,
    0x1956,
    0x448b,
    [0xa4, 0xeb, 0xae, 0xe6, 0x8b, 0xde, 0xb2, 0x83],
);
/// DXMT's `IMTLD3D11ContextExt1`.
pub static IID_CONTEXT_EXT1_11: Guid = Guid::new(
    0x19a8_e35a,
    0x38be,
    0x418f,
    [0x94, 0xe3, 0x9f, 0x73, 0x23, 0x93, 0x68, 0x70],
);

/// `ID3D12GraphicsCommandList` up to `GetDevice` (`ID3D12DeviceChild`'s).
#[repr(C)]
pub struct CommandList12Vtbl {
    pub unknown: IUnknownVtbl,
    _object: [*const c_void; 4],
    pub get_device:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
}

/// `D3D12_RESOURCE_DESC`.
#[repr(C)]
#[derive(Default)]
pub struct ResourceDesc12 {
    pub dimension: u32,
    pub alignment: u64,
    pub width: u64,
    pub height: u32,
    pub depth_or_array_size: u16,
    pub mip_levels: u16,
    pub format: u32,
    pub sample_count: u32,
    pub sample_quality: u32,
    pub layout: u32,
    pub flags: u32,
}

/// `ID3D12Resource` up to `GetDesc`, whose struct return comes back through a pointer after `this`.
#[repr(C)]
pub struct Resource12Vtbl {
    pub unknown: IUnknownVtbl,
    _object_child_map_unmap: [*const c_void; 7],
    pub get_desc:
        unsafe extern "system" fn(*mut c_void, *mut ResourceDesc12) -> *mut ResourceDesc12,
}

/// `MTL_TEMPORAL_SCALER_D3D12_DESC`.
#[repr(C)]
#[derive(Default, PartialEq)]
pub struct ScalerDesc12 {
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub color_format: u32,
    pub depth_format: u32,
    pub motion_format: u32,
    pub output_format: u32,
    pub reactive_mask_format: u32,
    pub auto_exposure: i32,
    pub output_resolution_motion_vectors: i32,
    pub jittered_motion_vectors: i32,
    pub input_content_min_scale: f32,
    pub input_content_max_scale: f32,
}

/// `MTL_TEMPORAL_UPSCALE_D3D12_DESC`.
#[repr(C)]
pub struct UpscaleDesc12 {
    pub color: *mut c_void,
    pub depth: *mut c_void,
    pub motion_vector: *mut c_void,
    pub exposure: *mut c_void,
    pub reactive_mask: *mut c_void,
    pub output: *mut c_void,
    pub input_content_width: u32,
    pub input_content_height: u32,
    pub color_offset: [u32; 2],
    pub depth_offset: [u32; 2],
    pub motion_offset: [u32; 2],
    pub reactive_offset: [u32; 2],
    pub output_offset: [u32; 2],
    pub jitter_offset: [f32; 2],
    pub motion_vector_scale: [f32; 2],
    pub pre_exposure: f32,
    pub reset: i32,
    pub depth_reversed: i32,
}

/// DXMT's `IMTLD3D12DeviceExt`.
#[repr(C)]
pub struct DeviceExt12Vtbl {
    pub unknown: IUnknownVtbl,
    pub get_temporal_scaler_scale_range:
        unsafe extern "system" fn(*mut c_void, *mut f32, *mut f32) -> Hresult,
    pub create_temporal_scaler:
        unsafe extern "system" fn(*mut c_void, *const ScalerDesc12, *mut *mut c_void) -> Hresult,
}

/// DXMT's `IMTLD3D12CommandListExt`.
#[repr(C)]
pub struct CommandListExt12Vtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *const UpscaleDesc12) -> Hresult,
}

/// `MTL_TEMPORAL_UPSCALE_D3D11_DESC`.
#[repr(C)]
pub struct UpscaleDesc11 {
    pub input_content_width: u32,
    pub input_content_height: u32,
    pub auto_exposure: i32,
    pub in_reset: i32,
    pub depth_reversed: i32,
    pub motion_vector_in_display_res: i32,
    pub color: *mut c_void,
    pub depth: *mut c_void,
    pub motion_vector: *mut c_void,
    pub output: *mut c_void,
    pub motion_vector_scale: [f32; 2],
    pub pre_exposure: f32,
    pub exposure_texture: *mut c_void,
    pub jitter_offset: [f32; 2],
}

/// `MTL_FEATURE_METALFX_TEMPORAL_SCALER`.
pub const FEATURE_METALFX_TEMPORAL_SCALER: u32 = 0;

/// DXMT's `IMTLD3D11ContextExt1` (with `IMTLD3D11ContextExt`'s slots first).
#[repr(C)]
pub struct ContextExt11Vtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale: unsafe extern "system" fn(*mut c_void, *const UpscaleDesc11),
    _uav_overlap: [*const c_void; 2],
    pub check_feature_support:
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u32) -> Hresult,
}

const _: () = {
    use core::mem::offset_of;
    let slot = size_of::<usize>();
    assert!(size_of::<ResourceDesc12>() == 56 && offset_of!(ResourceDesc12, width) == 16);
    assert!(offset_of!(ResourceDesc12, height) == 24 && offset_of!(ResourceDesc12, format) == 32);
    assert!(size_of::<ScalerDesc12>() == 56 && offset_of!(ScalerDesc12, color_format) == 16);
    assert!(
        offset_of!(ScalerDesc12, auto_exposure) == 36
            && offset_of!(ScalerDesc12, input_content_min_scale) == 48
    );
    assert!(
        size_of::<UpscaleDesc12>() == 128 && offset_of!(UpscaleDesc12, input_content_width) == 48
    );
    assert!(
        offset_of!(UpscaleDesc12, color_offset) == 56
            && offset_of!(UpscaleDesc12, jitter_offset) == 96
    );
    assert!(offset_of!(UpscaleDesc12, reset) == 116);
    assert!(size_of::<UpscaleDesc11>() == 88 && offset_of!(UpscaleDesc11, color) == 24);
    assert!(
        offset_of!(UpscaleDesc11, motion_vector_scale) == 56
            && offset_of!(UpscaleDesc11, exposure_texture) == 72
    );
    assert!(offset_of!(UpscaleDesc11, jitter_offset) == 80);
    assert!(offset_of!(CommandList12Vtbl, get_device) == 7 * slot);
    assert!(offset_of!(Resource12Vtbl, get_desc) == 10 * slot);
    assert!(offset_of!(ContextExt11Vtbl, check_feature_support) == 6 * slot);
    assert!(offset_of!(DeviceExt12Vtbl, create_temporal_scaler) == 4 * slot);
};
