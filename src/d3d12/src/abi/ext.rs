//! Private DXMT D3D12 and DXGI companion vtables (`d3d12_interfaces.hpp`, `dxgi_interfaces.h`).
//!
//! `dxmt_util::ext` defines the caller-side (`extern "system"`) vtables that `nvngx.dll`,
//! `nvapi64.dll`, and `dxgi.dll` invoke. `d3d12.dll` implements these interfaces, so its vtable
//! slots use `extern "system-unwind"` (`crate::com::slot!`) and are layout-asserted equal to
//! `dxmt_util::ext`'s.

use core::{
    ffi::c_void,
    mem::{align_of, offset_of, size_of},
};

pub use dxmt_util::ext::{
    D3D12ScalerDesc, D3D12UpscaleDesc, Hwnd, IID_D3D12_COMMAND_LIST_EXT, IID_D3D12_DEVICE_EXT,
    IID_D3D12_TEMPORAL_SCALER, IID_IMTL_DXGI_ADAPTER, IID_IMTL_SWAP_CHAIN_FACTORY,
    IMTLDXGIAdapterVtbl, Luid, Rational, Rect, SampleDesc, SwapChainDesc, SwapChainDesc1,
    SwapChainFullscreenDesc,
};
use dxmt_util::{
    com::{Guid, Hresult},
    ext as util_ext,
};

use crate::abi::vtbl::IUnknownVtbl;

/// `IMTLD3D12DeviceExt` vtable (`d3d12_interfaces.hpp`).
#[repr(C)]
pub struct D3D12DeviceExtVtbl {
    pub unknown: IUnknownVtbl,
    pub get_temporal_scaler_scale_range: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        min: *mut f32,
        max: *mut f32,
    ) -> Hresult,
    pub create_temporal_scaler: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        desc: *const D3D12ScalerDesc,
        out: *mut *mut c_void,
    ) -> Hresult,
}

/// `IMTLD3D12CommandListExt` vtable (`d3d12_interfaces.hpp`).
#[repr(C)]
pub struct D3D12CommandListExtVtbl {
    pub unknown: IUnknownVtbl,
    pub temporal_upscale: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        scaler: *mut c_void,
        desc: *const D3D12UpscaleDesc,
    ) -> Hresult,
}

/// `IMTLD3D12TemporalScaler` vtable (`d3d12_interfaces.hpp`, `IUnknown` only).
#[repr(C)]
pub struct D3D12TemporalScalerVtbl {
    pub unknown: IUnknownVtbl,
}

/// `IMTLSwapChainFactory` vtable (`dxgi_interfaces.h`, implemented on D3D12's command queue).
#[repr(C)]
pub struct IMTLSwapChainFactoryVtbl {
    pub unknown: IUnknownVtbl,
    pub create_swap_chain: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        factory: *mut c_void,
        hwnd: Hwnd,
        desc: *const SwapChainDesc1,
        fullscreen_desc: *const SwapChainFullscreenDesc,
        out: *mut *mut c_void,
    ) -> Hresult,
}

/// Downlevel device query UUID (`d3d12_device.cpp:40`), refused silently without a warning.
pub const IID_D3D12_DEVICE_DOWNLEVEL: Guid = Guid::new(
    0x74ea_ee3f,
    0x2f4b,
    0x476d,
    [0x82, 0xba, 0x2b, 0x85, 0xcb, 0x49, 0xe3, 0x10],
);

/// Downlevel command-queue query UUID (`d3d12_command_queue.cpp:40`), refused silently.
pub const IID_D3D12_COMMAND_QUEUE_DOWNLEVEL: Guid = Guid::new(
    0x38a8_c5ef,
    0x7ccb,
    0x4e81,
    [0x91, 0x4f, 0xa6, 0xe9, 0xd0, 0x72, 0xc4, 0x94],
);

const _: () = {
    assert!(size_of::<D3D12DeviceExtVtbl>() == size_of::<util_ext::D3D12DeviceExtVtbl>());
    assert!(align_of::<D3D12DeviceExtVtbl>() == align_of::<util_ext::D3D12DeviceExtVtbl>());
    assert!(
        offset_of!(D3D12DeviceExtVtbl, get_temporal_scaler_scale_range)
            == offset_of!(
                util_ext::D3D12DeviceExtVtbl,
                get_temporal_scaler_scale_range
            )
    );
    assert!(
        offset_of!(D3D12DeviceExtVtbl, create_temporal_scaler)
            == offset_of!(util_ext::D3D12DeviceExtVtbl, create_temporal_scaler)
    );

    assert!(size_of::<D3D12CommandListExtVtbl>() == size_of::<util_ext::D3D12CommandListExtVtbl>());
    assert!(
        align_of::<D3D12CommandListExtVtbl>() == align_of::<util_ext::D3D12CommandListExtVtbl>()
    );
    assert!(
        offset_of!(D3D12CommandListExtVtbl, temporal_upscale)
            == offset_of!(util_ext::D3D12CommandListExtVtbl, temporal_upscale)
    );

    assert!(size_of::<D3D12TemporalScalerVtbl>() == 3 * size_of::<usize>());
    assert!(align_of::<D3D12TemporalScalerVtbl>() == align_of::<usize>());

    assert!(
        size_of::<IMTLSwapChainFactoryVtbl>() == size_of::<util_ext::IMTLSwapChainFactoryVtbl>()
    );
    assert!(
        align_of::<IMTLSwapChainFactoryVtbl>() == align_of::<util_ext::IMTLSwapChainFactoryVtbl>()
    );
    assert!(
        offset_of!(IMTLSwapChainFactoryVtbl, create_swap_chain)
            == offset_of!(util_ext::IMTLSwapChainFactoryVtbl, create_swap_chain)
    );

    assert!(size_of::<D3D12ScalerDesc>() == 56);
    assert!(align_of::<D3D12ScalerDesc>() == 4);
    assert!(offset_of!(D3D12ScalerDesc, input_width) == 0);
    assert!(offset_of!(D3D12ScalerDesc, input_height) == 4);
    assert!(offset_of!(D3D12ScalerDesc, output_width) == 8);
    assert!(offset_of!(D3D12ScalerDesc, output_height) == 12);
    assert!(offset_of!(D3D12ScalerDesc, color_format) == 16);
    assert!(offset_of!(D3D12ScalerDesc, depth_format) == 20);
    assert!(offset_of!(D3D12ScalerDesc, motion_format) == 24);
    assert!(offset_of!(D3D12ScalerDesc, output_format) == 28);
    assert!(offset_of!(D3D12ScalerDesc, reactive_mask_format) == 32);
    assert!(offset_of!(D3D12ScalerDesc, auto_exposure) == 36);
    assert!(offset_of!(D3D12ScalerDesc, output_resolution_motion_vectors) == 40);
    assert!(offset_of!(D3D12ScalerDesc, jittered_motion_vectors) == 44);
    assert!(offset_of!(D3D12ScalerDesc, input_content_min_scale) == 48);
    assert!(offset_of!(D3D12ScalerDesc, input_content_max_scale) == 52);
};

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<D3D12UpscaleDesc>() == 128);
    assert!(align_of::<D3D12UpscaleDesc>() == 8);
    assert!(offset_of!(D3D12UpscaleDesc, color) == 0);
    assert!(offset_of!(D3D12UpscaleDesc, depth) == 8);
    assert!(offset_of!(D3D12UpscaleDesc, motion_vector) == 16);
    assert!(offset_of!(D3D12UpscaleDesc, exposure) == 24);
    assert!(offset_of!(D3D12UpscaleDesc, reactive_mask) == 32);
    assert!(offset_of!(D3D12UpscaleDesc, output) == 40);
    assert!(offset_of!(D3D12UpscaleDesc, input_content_width) == 48);
    assert!(offset_of!(D3D12UpscaleDesc, input_content_height) == 52);
    assert!(offset_of!(D3D12UpscaleDesc, color_offset) == 56);
    assert!(offset_of!(D3D12UpscaleDesc, depth_offset) == 64);
    assert!(offset_of!(D3D12UpscaleDesc, motion_offset) == 72);
    assert!(offset_of!(D3D12UpscaleDesc, reactive_offset) == 80);
    assert!(offset_of!(D3D12UpscaleDesc, output_offset) == 88);
    assert!(offset_of!(D3D12UpscaleDesc, jitter_offset) == 96);
    assert!(offset_of!(D3D12UpscaleDesc, motion_vector_scale) == 104);
    assert!(offset_of!(D3D12UpscaleDesc, pre_exposure) == 112);
    assert!(offset_of!(D3D12UpscaleDesc, reset) == 116);
    assert!(offset_of!(D3D12UpscaleDesc, depth_reversed) == 120);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<D3D12UpscaleDesc>() == 100);
    assert!(align_of::<D3D12UpscaleDesc>() == 4);
    assert!(offset_of!(D3D12UpscaleDesc, color) == 0);
    assert!(offset_of!(D3D12UpscaleDesc, depth) == 4);
    assert!(offset_of!(D3D12UpscaleDesc, motion_vector) == 8);
    assert!(offset_of!(D3D12UpscaleDesc, exposure) == 12);
    assert!(offset_of!(D3D12UpscaleDesc, reactive_mask) == 16);
    assert!(offset_of!(D3D12UpscaleDesc, output) == 20);
    assert!(offset_of!(D3D12UpscaleDesc, input_content_width) == 24);
    assert!(offset_of!(D3D12UpscaleDesc, input_content_height) == 28);
    assert!(offset_of!(D3D12UpscaleDesc, color_offset) == 32);
    assert!(offset_of!(D3D12UpscaleDesc, depth_offset) == 40);
    assert!(offset_of!(D3D12UpscaleDesc, motion_offset) == 48);
    assert!(offset_of!(D3D12UpscaleDesc, reactive_offset) == 56);
    assert!(offset_of!(D3D12UpscaleDesc, output_offset) == 64);
    assert!(offset_of!(D3D12UpscaleDesc, jitter_offset) == 72);
    assert!(offset_of!(D3D12UpscaleDesc, motion_vector_scale) == 80);
    assert!(offset_of!(D3D12UpscaleDesc, pre_exposure) == 88);
    assert!(offset_of!(D3D12UpscaleDesc, reset) == 92);
    assert!(offset_of!(D3D12UpscaleDesc, depth_reversed) == 96);
};
