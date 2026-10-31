//! DXMT's private D3D11 and D3D12 interfaces: their IIDs, vtables and the structs they take.
//!
//! `d3d11_interfaces.hpp` and `d3d12_interfaces.hpp` are the C++ side, each declaring what a
//! game's device, context or command list answers `QueryInterface` with when DXMT is the D3D
//! runtime. NGX and NVAPI query them, and the Rust D3D11 and D3D12 will implement them. Layouts
//! are the C++ headers' on every target, checked by the asserts below; a vtable lists the slots
//! after `IUnknown`'s three in declaration order, a derived interface's after its base's.

use core::{ffi::c_void, mem::offset_of};

use crate::com::{Guid, Hresult, IUnknownVtbl};

/// DXMT's `IMTLD3D11ContextExt`.
pub static IID_D3D11_CONTEXT_EXT: Guid = Guid::new(
    0x43ac_e3ce,
    0x1956,
    0x448b,
    [0xa4, 0xeb, 0xae, 0xe6, 0x8b, 0xde, 0xb2, 0x83],
);
/// DXMT's `IMTLD3D11ContextExt1`.
pub static IID_D3D11_CONTEXT_EXT1: Guid = Guid::new(
    0x19a8_e35a,
    0x38be,
    0x418f,
    [0x94, 0xe3, 0x9f, 0x73, 0x23, 0x93, 0x68, 0x70],
);
/// DXMT's `IMTLD3D11ContextExt2`.
pub static IID_D3D11_CONTEXT_EXT2: Guid = Guid::new(
    0xa525_3c74,
    0x3ff9,
    0x497f,
    [0xb1, 0x84, 0xf1, 0x37, 0xc5, 0xa7, 0xb9, 0x28],
);
/// DXMT's `IMTLD3D11DeviceExt`.
pub static IID_D3D11_DEVICE_EXT: Guid = Guid::new(
    0xefc7_7ae6,
    0x2179,
    0x4c0a,
    [0xb8, 0x44, 0x76, 0x61, 0xca, 0x0d, 0xcd, 0xe7],
);
/// DXMT's `IMTLD3D12TemporalScaler`.
pub static IID_D3D12_TEMPORAL_SCALER: Guid = Guid::new(
    0x5b27_acde,
    0xa0d8,
    0x4677,
    [0x93, 0xc4, 0xee, 0xdd, 0x17, 0xa0, 0xab, 0x14],
);
/// DXMT's `IMTLD3D12DeviceExt`.
pub static IID_D3D12_DEVICE_EXT: Guid = Guid::new(
    0xa8c6_4ba7,
    0x6d54,
    0x4833,
    [0x8c, 0xc6, 0x26, 0x05, 0xc5, 0xb4, 0x54, 0xbe],
);
/// DXMT's `IMTLD3D12CommandListExt`.
pub static IID_D3D12_COMMAND_LIST_EXT: Guid = Guid::new(
    0x5690_e8a7,
    0x51a1,
    0x4b37,
    [0x8b, 0xbd, 0x73, 0x0d, 0x0f, 0xdb, 0x2c, 0xec],
);

/// `MTL_FEATURE_METALFX_TEMPORAL_SCALER`, the feature `CheckFeatureSupport` answers.
pub const FEATURE_METALFX_TEMPORAL_SCALER: u32 = 0;

/// `MTL_TEMPORAL_UPSCALE_D3D11_DESC`.
#[repr(C)]
pub struct D3D11UpscaleDesc {
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

/// `MTLD3D11Piece`: a piece of a dynamic resource's memory `GetPieces` lends.
#[repr(C)]
pub struct D3D11Piece {
    pub id: u64,
    pub cpu: *mut c_void,
    pub row_pitch: u32,
    pub depth_pitch: u32,
}

/// `IMTLD3D11ContextExt`: `TemporalUpscale`, `BeginUAVOverlap` and `EndUAVOverlap`.
#[repr(C)]
pub struct D3D11ContextExtVtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale: unsafe extern "system" fn(*mut c_void, *const D3D11UpscaleDesc),
    pub begin_uav_overlap: unsafe extern "system" fn(*mut c_void),
    pub end_uav_overlap: unsafe extern "system" fn(*mut c_void),
}

/// `IMTLD3D11ContextExt1`: `ContextExt`'s slots, then `CheckFeatureSupport`.
#[repr(C)]
pub struct D3D11ContextExt1Vtbl {
    pub base: D3D11ContextExtVtbl,
    pub check_feature_support:
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u32) -> Hresult,
}

/// `IMTLD3D11ContextExt2`: `ContextExt1`'s slots, then `GetPieces` and `UsePiece`.
#[repr(C)]
pub struct D3D11ContextExt2Vtbl {
    pub base: D3D11ContextExt1Vtbl,
    pub get_pieces:
        unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32, *mut D3D11Piece) -> Hresult,
    pub use_piece: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u64),
}

/// `IMTLD3D11DeviceExt`: `SetShaderExtensionSlot`.
#[repr(C)]
pub struct D3D11DeviceExtVtbl {
    pub unknown: IUnknownVtbl,
    pub set_shader_extension_slot: unsafe extern "system" fn(*mut c_void, u32),
}

/// `MTL_TEMPORAL_SCALER_D3D12_DESC`.
#[repr(C)]
#[derive(Default, PartialEq)]
pub struct D3D12ScalerDesc {
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
pub struct D3D12UpscaleDesc {
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

/// `IMTLD3D12DeviceExt`: the temporal scaler's range and its creation.
#[repr(C)]
pub struct D3D12DeviceExtVtbl {
    pub unknown: IUnknownVtbl,
    pub get_temporal_scaler_scale_range:
        unsafe extern "system" fn(*mut c_void, *mut f32, *mut f32) -> Hresult,
    pub create_temporal_scaler:
        unsafe extern "system" fn(*mut c_void, *const D3D12ScalerDesc, *mut *mut c_void) -> Hresult,
}

/// `IMTLD3D12CommandListExt`: `TemporalUpscale`.
#[repr(C)]
pub struct D3D12CommandListExtVtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *const D3D12UpscaleDesc) -> Hresult,
}

const _: () = {
    let slot = size_of::<usize>();
    let wide = slot == 8;
    // The descriptions hold pointers: their offsets after the first one move with the width.
    assert!(offset_of!(D3D11UpscaleDesc, color) == 24);
    assert!(offset_of!(D3D11UpscaleDesc, motion_vector_scale) == 24 + 4 * slot);
    assert!(offset_of!(D3D11UpscaleDesc, exposure_texture) == if wide { 72 } else { 52 });
    assert!(offset_of!(D3D11UpscaleDesc, jitter_offset) == if wide { 80 } else { 56 });
    assert!(size_of::<D3D11UpscaleDesc>() == if wide { 88 } else { 64 });
    assert!(offset_of!(D3D11Piece, cpu) == 8);
    assert!(offset_of!(D3D11Piece, row_pitch) == 8 + slot && size_of::<D3D11Piece>() == 24);
    assert!(size_of::<D3D12ScalerDesc>() == 56 && offset_of!(D3D12ScalerDesc, color_format) == 16);
    assert!(
        offset_of!(D3D12ScalerDesc, auto_exposure) == 36
            && offset_of!(D3D12ScalerDesc, input_content_min_scale) == 48
    );
    assert!(offset_of!(D3D12UpscaleDesc, input_content_width) == 6 * slot);
    assert!(offset_of!(D3D12UpscaleDesc, color_offset) == 6 * slot + 8);
    assert!(offset_of!(D3D12UpscaleDesc, jitter_offset) == 6 * slot + 48);
    assert!(offset_of!(D3D12UpscaleDesc, reset) == 6 * slot + 68);
    assert!(size_of::<D3D12UpscaleDesc>() == if wide { 128 } else { 100 });
    assert!(offset_of!(D3D11ContextExtVtbl, temporal_upscale) == 3 * slot);
    assert!(offset_of!(D3D11ContextExtVtbl, end_uav_overlap) == 5 * slot);
    assert!(offset_of!(D3D11ContextExt1Vtbl, check_feature_support) == 6 * slot);
    assert!(offset_of!(D3D11ContextExt2Vtbl, get_pieces) == 7 * slot);
    assert!(offset_of!(D3D11ContextExt2Vtbl, use_piece) == 8 * slot);
    assert!(offset_of!(D3D11DeviceExtVtbl, set_shader_extension_slot) == 3 * slot);
    assert!(offset_of!(D3D12DeviceExtVtbl, create_temporal_scaler) == 4 * slot);
    assert!(offset_of!(D3D12CommandListExtVtbl, temporal_upscale) == 3 * slot);
};

#[cfg(test)]
mod tests;
