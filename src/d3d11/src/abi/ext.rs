//! DXMT's private D3D11 and DXGI companion interfaces (`crate::abi::ext`).
//!
//! Layout-asserted against [`dxmt_util::ext`]; the vtables use `extern "system-unwind"` (P20)
//! to match `crate::abi::vtbl`.

use core::{
    ffi::c_void,
    mem::{align_of, offset_of, size_of},
};

pub use dxmt_util::ext::{
    DXMT_NVEXT_GUID, FEATURE_METALFX_TEMPORAL_SCALER, Hmonitor, Hwnd, IID_D3D11_CONTEXT_EXT,
    IID_D3D11_CONTEXT_EXT1, IID_D3D11_CONTEXT_EXT2, IID_D3D11_DEVICE_EXT, IID_IMTL_DXGI_ADAPTER,
    IID_IMTL_DXGI_DEVICE, IID_IMTL_DXGI_OUTPUT, IID_IMTL_SWAP_CHAIN_FACTORY, IMTLDXGIAdapterVtbl,
    IMTLDXGIOutputVtbl, Luid, ModeDesc, ModeDesc1, OutputDesc, OutputDesc1, Rational, Rect,
    SampleDesc, SwapChainDesc, SwapChainDesc1, SwapChainFullscreenDesc, Wchar,
};

use super::vtbl::{ID3D11DeviceContext4Vtbl, IUnknownVtbl};
use crate::com::{Guid, Hresult};

/// `IMTLMinLODClampable`'s IID (`d3d11_resource.hpp:132`): `102d11a1-5be7-46c9-a19a-56b036c26f78`.
pub const IID_IMTL_MIN_LOD_CLAMPABLE: Guid = Guid::new(
    0x102d_11a1,
    0x5be7,
    0x46c9,
    [0xa1, 0x9a, 0x56, 0xb0, 0x36, 0xc2, 0x6f, 0x78],
);

/// `IMTLMinLODClampable`'s 2 slots (`d3d11_resource.hpp:126-130`).
#[repr(C)]
pub struct IMTLMinLODClampableVtbl {
    pub set_min_lod: unsafe extern "system-unwind" fn(*mut c_void, f32),
    pub get_min_lod: unsafe extern "system-unwind" fn(*mut c_void) -> f32,
}

/// `IMTLD3D11DeviceContext`: `ID3D11DeviceContext4` plus 6 internal slots (`d3d11_device.hpp`).
#[repr(C)]
pub struct IMTLD3D11DeviceContextVtbl {
    pub base: ID3D11DeviceContext4Vtbl,
    pub wait_until_gpu_idle: unsafe extern "system-unwind" fn(*mut c_void),
    pub prepare_flush: unsafe extern "system-unwind" fn(*mut c_void),
    pub commit: unsafe extern "system-unwind" fn(*mut c_void),
    pub so_get_targets_with_offsets:
        unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void, *mut u32),
    pub acquire_sync: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u64),
    pub release_sync: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u64),
}

/// `MTL_TEMPORAL_UPSCALE_D3D11_DESC` (`d3d11_interfaces.hpp:15-30`).
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

/// `MTLD3D11Piece` (`d3d11_interfaces.hpp:36-41`).
#[repr(C)]
pub struct D3D11Piece {
    pub id: u64,
    pub cpu: *mut c_void,
    pub row_pitch: u32,
    pub depth_pitch: u32,
}

/// `IMTLD3D11ContextExt`'s 6 slots (`d3d11_interfaces.hpp:44-50`).
#[repr(C)]
pub struct IMTLD3D11ContextExtVtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale: unsafe extern "system-unwind" fn(*mut c_void, *const D3D11UpscaleDesc),
    pub begin_uav_overlap: unsafe extern "system-unwind" fn(*mut c_void),
    pub end_uav_overlap: unsafe extern "system-unwind" fn(*mut c_void),
}

/// `IMTLD3D11ContextExt1`'s 7 slots (`d3d11_interfaces.hpp:52-55`).
#[repr(C)]
pub struct IMTLD3D11ContextExt1Vtbl {
    pub base: IMTLD3D11ContextExtVtbl,
    pub check_feature_support:
        unsafe extern "system-unwind" fn(*mut c_void, u32, *mut c_void, u32) -> Hresult,
}

/// `IMTLD3D11ContextExt2`'s 9 slots (`d3d11_interfaces.hpp:58-63`).
#[repr(C)]
pub struct IMTLD3D11ContextExt2Vtbl {
    pub base: IMTLD3D11ContextExt1Vtbl,
    pub get_pieces: unsafe extern "system-unwind" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        *mut D3D11Piece,
    ) -> Hresult,
    pub use_piece: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32, u64),
}

/// `IMTLD3D11DeviceExt`'s 4 slots (`d3d11_interfaces.hpp:66-70`).
#[repr(C)]
pub struct IMTLD3D11DeviceExtVtbl {
    pub unknown: IUnknownVtbl,
    pub set_shader_extension_slot: unsafe extern "system-unwind" fn(*mut c_void, u32),
}

/// `IMTLSwapChainFactory`'s 4 slots (`dxgi_interfaces.h`).
#[repr(C)]
pub struct IMTLSwapChainFactoryVtbl {
    pub unknown: IUnknownVtbl,
    pub create_swap_chain: unsafe extern "system-unwind" fn(
        *mut c_void,
        *mut c_void,
        Hwnd,
        *const SwapChainDesc1,
        *const SwapChainFullscreenDesc,
        *mut *mut c_void,
    ) -> Hresult,
}

const _: () = {
    use dxmt_util::ext as u;

    assert!(size_of::<D3D11UpscaleDesc>() == size_of::<u::D3D11UpscaleDesc>());
    assert!(align_of::<D3D11UpscaleDesc>() == align_of::<u::D3D11UpscaleDesc>());
    assert!(offset_of!(D3D11UpscaleDesc, color) == offset_of!(u::D3D11UpscaleDesc, color));
    assert!(
        offset_of!(D3D11UpscaleDesc, motion_vector_scale)
            == offset_of!(u::D3D11UpscaleDesc, motion_vector_scale)
    );
    assert!(
        offset_of!(D3D11UpscaleDesc, exposure_texture)
            == offset_of!(u::D3D11UpscaleDesc, exposure_texture)
    );
    assert!(
        offset_of!(D3D11UpscaleDesc, jitter_offset)
            == offset_of!(u::D3D11UpscaleDesc, jitter_offset)
    );

    assert!(size_of::<D3D11Piece>() == size_of::<u::D3D11Piece>());
    assert!(align_of::<D3D11Piece>() == align_of::<u::D3D11Piece>());
    assert!(offset_of!(D3D11Piece, cpu) == offset_of!(u::D3D11Piece, cpu));
    assert!(offset_of!(D3D11Piece, row_pitch) == offset_of!(u::D3D11Piece, row_pitch));

    assert!(size_of::<IMTLD3D11ContextExtVtbl>() == size_of::<u::D3D11ContextExtVtbl>());
    assert!(
        offset_of!(IMTLD3D11ContextExtVtbl, temporal_upscale)
            == offset_of!(u::D3D11ContextExtVtbl, temporal_upscale)
    );
    assert!(size_of::<IMTLD3D11ContextExt1Vtbl>() == size_of::<u::D3D11ContextExt1Vtbl>());
    assert!(
        offset_of!(IMTLD3D11ContextExt1Vtbl, check_feature_support)
            == offset_of!(u::D3D11ContextExt1Vtbl, check_feature_support)
    );
    assert!(size_of::<IMTLD3D11ContextExt2Vtbl>() == size_of::<u::D3D11ContextExt2Vtbl>());
    assert!(
        offset_of!(IMTLD3D11ContextExt2Vtbl, get_pieces)
            == offset_of!(u::D3D11ContextExt2Vtbl, get_pieces)
    );
    assert!(
        offset_of!(IMTLD3D11ContextExt2Vtbl, use_piece)
            == offset_of!(u::D3D11ContextExt2Vtbl, use_piece)
    );
    assert!(size_of::<IMTLD3D11DeviceExtVtbl>() == size_of::<u::D3D11DeviceExtVtbl>());
    assert!(
        offset_of!(IMTLD3D11DeviceExtVtbl, set_shader_extension_slot)
            == offset_of!(u::D3D11DeviceExtVtbl, set_shader_extension_slot)
    );
    assert!(size_of::<IMTLSwapChainFactoryVtbl>() == size_of::<u::IMTLSwapChainFactoryVtbl>());
    assert!(
        offset_of!(IMTLSwapChainFactoryVtbl, create_swap_chain)
            == offset_of!(u::IMTLSwapChainFactoryVtbl, create_swap_chain)
    );
    assert!(size_of::<IMTLD3D11DeviceContextVtbl>() == 155 * size_of::<usize>());
};
