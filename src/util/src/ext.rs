//! DXMT's private DXGI, D3D11 and D3D12 interfaces: their IIDs, vtables and the structs they take.
//!
//! `dxgi_interfaces.h`, `d3d11_interfaces.hpp` and `d3d12_interfaces.hpp` are the C++ side, each
//! declaring what an adapter, output, device, context or command list answers `QueryInterface` with
//! when DXMT is the runtime. DXGI, NGX, NVAPI, D3D11 and D3D12 query or implement them. Layouts are
//! the C++ headers' on every target, checked by the asserts below; a vtable lists the slots after
//! `IUnknown`'s three in declaration order, a derived interface's after its base's.

use core::{ffi::c_void, mem::offset_of};

use crate::{
    GammaRamp,
    com::{Guid, Hresult, IUnknownVtbl},
};

/// `WCHAR`: UTF-16 on PE.
#[cfg(windows)]
pub type Wchar = u16;
/// `WCHAR`: natively `wchar_t`, UTF-32.
#[cfg(not(windows))]
pub type Wchar = u32;
/// `HWND`.
pub type Hwnd = *mut c_void;
/// `HMONITOR`.
pub type Hmonitor = *mut c_void;

/// DXMT's `IMTLDXGIAdapter`: `IDXGIAdapter4`, then `GetMTLDevice` and `GetLocalD3DKMT`.
pub const IID_IMTL_DXGI_ADAPTER: Guid = Guid::new(
    0xacdf_3ef1,
    0xb33a,
    0x4cb6,
    [0x97, 0xbd, 0x1c, 0x19, 0x74, 0x82, 0x7e, 0x6d],
);
/// DXMT's `IMTLDXGIOutput`: `IDXGIOutput6`, then `GetGammaRamp`.
pub const IID_IMTL_DXGI_OUTPUT: Guid = Guid::new(
    0x4c11_28cd,
    0xb437,
    0x4d44,
    [0x99, 0x58, 0x5d, 0x76, 0x5b, 0xb8, 0x5a, 0x76],
);
/// DXMT's `IMTLDXGIDevice` (D3D11's DXGI device).
pub const IID_IMTL_DXGI_DEVICE: Guid = Guid::new(
    0x6bfa_1657,
    0x9cb1,
    0x471a,
    [0xa4, 0xfb, 0x7c, 0xac, 0xf8, 0xa8, 0x12, 0x07],
);
/// DXMT's `IMTLSwapChainFactory`, which a D3D11 or D3D12 device answers.
pub const IID_IMTL_SWAP_CHAIN_FACTORY: Guid = Guid::new(
    0x6c03_0003,
    0x460a,
    0x4b19,
    [0x90, 0x9b, 0x38, 0xf9, 0x7d, 0x20, 0x3e, 0x45],
);
/// DXMT's NVIDIA extension, which `DXGIGetDebugInterface1` answers when it is on.
pub const DXMT_NVEXT_GUID: Guid = Guid::new(
    0xba0a_f616,
    0x4a43,
    0x4259,
    [0x81, 0x5c, 0xdb, 0x3b, 0x89, 0x82, 0x99, 0x05],
);

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

/// `IUnknown` and `IDXGIObject`'s slots, which the factory, adapter and output start with.
#[repr(C)]
pub struct ObjectVtbl {
    pub unknown: IUnknownVtbl,
    pub set_private_data:
        unsafe extern "system" fn(*mut c_void, *const Guid, u32, *const c_void) -> Hresult,
    pub set_private_data_interface:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut c_void) -> Hresult,
    pub get_private_data:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut u32, *mut c_void) -> Hresult,
    pub get_parent:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
}

/// `IMTLDXGIAdapter`'s 21 slots: `IDXGIAdapter4`'s 19, then DXMT's two.
#[repr(C)]
pub struct IMTLDXGIAdapterVtbl {
    pub object: ObjectVtbl,
    pub enum_outputs: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> Hresult,
    pub get_desc: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc) -> Hresult,
    pub check_interface_support:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut i64) -> Hresult,
    pub get_desc1: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc1) -> Hresult,
    pub get_desc2: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc2) -> Hresult,
    pub register_hardware_content_protection_teardown_status_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_hardware_content_protection_teardown_status:
        unsafe extern "system" fn(*mut c_void, u32),
    pub query_video_memory_info:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut QueryVideoMemoryInfo) -> Hresult,
    pub set_video_memory_reservation:
        unsafe extern "system" fn(*mut c_void, u32, u32, u64) -> Hresult,
    pub register_video_memory_budget_change_notification_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_video_memory_budget_change_notification:
        unsafe extern "system" fn(*mut c_void, u32),
    pub get_desc3: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc3) -> Hresult,
    /// The Metal device, a winemetal handle the adapter keeps retained.
    pub get_mtl_device: unsafe extern "system" fn(*mut c_void) -> u64,
    pub get_local_d3dkmt: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// `IMTLDXGIAdapterVtbl` under `dxgi`'s shorter name.
pub type AdapterVtbl = IMTLDXGIAdapterVtbl;

/// `IMTLDXGIOutput`'s 30 slots: `IDXGIOutput6`'s 29, then `GetGammaRamp`.
#[repr(C)]
pub struct IMTLDXGIOutputVtbl {
    pub object: ObjectVtbl,
    pub get_desc: unsafe extern "system" fn(*mut c_void, *mut OutputDesc) -> Hresult,
    pub get_display_mode_list:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut u32, *mut ModeDesc) -> Hresult,
    pub find_closest_matching_mode: unsafe extern "system" fn(
        *mut c_void,
        *const ModeDesc,
        *mut ModeDesc,
        *mut c_void,
    ) -> Hresult,
    pub wait_for_v_blank: unsafe extern "system" fn(*mut c_void) -> Hresult,
    pub take_ownership: unsafe extern "system" fn(*mut c_void, *mut c_void, i32) -> Hresult,
    pub release_ownership: unsafe extern "system" fn(*mut c_void),
    pub get_gamma_control_capabilities:
        unsafe extern "system" fn(*mut c_void, *mut GammaControlCapabilities) -> Hresult,
    pub set_gamma_control: unsafe extern "system" fn(*mut c_void, *const GammaControl) -> Hresult,
    pub get_gamma_control: unsafe extern "system" fn(*mut c_void, *mut GammaControl) -> Hresult,
    pub set_display_surface: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub get_display_surface_data: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    /// `GetFrameStatistics`: its `DXGI_FRAME_STATISTICS` is never written (a stub).
    pub get_frame_statistics: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub get_display_mode_list1:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut u32, *mut ModeDesc1) -> Hresult,
    pub find_closest_matching_mode1: unsafe extern "system" fn(
        *mut c_void,
        *const ModeDesc1,
        *mut ModeDesc1,
        *mut c_void,
    ) -> Hresult,
    pub get_display_surface_data1: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub duplicate_output:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> Hresult,
    pub supports_overlays: unsafe extern "system" fn(*mut c_void) -> i32,
    pub check_overlay_support:
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, *mut u32) -> Hresult,
    pub check_overlay_color_space_support:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut c_void, *mut u32) -> Hresult,
    pub duplicate_output1: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        *const u32,
        *mut *mut c_void,
    ) -> Hresult,
    pub get_desc1: unsafe extern "system" fn(*mut c_void, *mut OutputDesc1) -> Hresult,
    pub check_hardware_composition_support:
        unsafe extern "system" fn(*mut c_void, *mut u32) -> Hresult,
    /// The output's gamma ramp, which D3D11's swap chain reads at each fullscreen present.
    pub get_gamma_ramp: unsafe extern "system" fn(*mut c_void) -> *const GammaRamp,
}

/// `IMTLDXGIOutputVtbl` under `dxgi`'s shorter name.
pub type OutputVtbl = IMTLDXGIOutputVtbl;

/// `IMTLSwapChainFactory`, at a base of a D3D11 or D3D12 device that is not its first.
///
/// `CreateSwapChain` is C++ that may throw, so its slot unwinds.
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

/// `IMTLSwapChainFactoryVtbl` under `dxgi`'s shorter name.
pub type SwapChainFactoryVtbl = IMTLSwapChainFactoryVtbl;

/// `LUID`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Luid {
    pub low_part: u32,
    pub high_part: i32,
}

/// `RECT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `DXGI_RATIONAL`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

/// `DXGI_SAMPLE_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SampleDesc {
    pub count: u32,
    pub quality: u32,
}

/// `DXGI_RGB`.
#[repr(C)]
pub struct Rgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

/// `DXGI_ADAPTER_DESC`.
#[repr(C)]
pub struct AdapterDesc {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
}

/// `DXGI_ADAPTER_DESC1`.
#[repr(C)]
pub struct AdapterDesc1 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
}

/// `DXGI_ADAPTER_DESC2`.
#[repr(C)]
pub struct AdapterDesc2 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
    pub graphics_preemption_granularity: u32,
    pub compute_preemption_granularity: u32,
}

/// `DXGI_ADAPTER_DESC3`.
#[repr(C)]
pub struct AdapterDesc3 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
    pub graphics_preemption_granularity: u32,
    pub compute_preemption_granularity: u32,
}

/// `DXGI_OUTPUT_DESC`.
#[repr(C)]
pub struct OutputDesc {
    pub device_name: [Wchar; 32],
    pub desktop_coordinates: Rect,
    pub attached_to_desktop: i32,
    pub rotation: u32,
    pub monitor: Hmonitor,
}

/// `DXGI_OUTPUT_DESC1`.
#[repr(C)]
pub struct OutputDesc1 {
    pub device_name: [Wchar; 32],
    pub desktop_coordinates: Rect,
    pub attached_to_desktop: i32,
    pub rotation: u32,
    pub monitor: Hmonitor,
    pub bits_per_color: u32,
    pub color_space: u32,
    pub red_primary: [f32; 2],
    pub green_primary: [f32; 2],
    pub blue_primary: [f32; 2],
    pub white_point: [f32; 2],
    pub min_luminance: f32,
    pub max_luminance: f32,
    pub max_full_frame_luminance: f32,
}

/// `DXGI_MODE_DESC`.
#[repr(C)]
pub struct ModeDesc {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: Rational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
}

/// `DXGI_MODE_DESC1`.
#[derive(Default)]
#[repr(C)]
pub struct ModeDesc1 {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: Rational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub stereo: i32,
}

/// `DXGI_SWAP_CHAIN_DESC`.
#[repr(C)]
pub struct SwapChainDesc {
    pub buffer_desc: ModeDesc,
    pub sample_desc: SampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub output_window: Hwnd,
    pub windowed: i32,
    pub swap_effect: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_DESC1`.
#[repr(C)]
pub struct SwapChainDesc1 {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub stereo: i32,
    pub sample_desc: SampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub scaling: u32,
    pub swap_effect: u32,
    pub alpha_mode: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_FULLSCREEN_DESC`.
#[repr(C)]
pub struct SwapChainFullscreenDesc {
    pub refresh_rate: Rational,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub windowed: i32,
}

/// `DXGI_GAMMA_CONTROL`.
#[repr(C)]
pub struct GammaControl {
    pub scale: Rgb,
    pub offset: Rgb,
    pub gamma_curve: [Rgb; 1025],
}

/// `DXGI_GAMMA_CONTROL_CAPABILITIES`.
#[repr(C)]
pub struct GammaControlCapabilities {
    pub scale_and_offset_supported: i32,
    pub max_converted_value: f32,
    pub min_converted_value: f32,
    pub num_gamma_control_points: u32,
    pub control_point_positions: [f32; 1025],
}

/// `DXGI_QUERY_VIDEO_MEMORY_INFO`.
#[repr(C)]
pub struct QueryVideoMemoryInfo {
    pub budget: u64,
    pub current_usage: u64,
    pub available_for_reservation: u64,
    pub current_reservation: u64,
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
    assert!(offset_of!(IMTLDXGIAdapterVtbl, get_mtl_device) == 19 * slot);
    assert!(offset_of!(IMTLDXGIAdapterVtbl, get_local_d3dkmt) == 20 * slot);
    assert!(size_of::<IMTLDXGIAdapterVtbl>() == 21 * slot);
    assert!(offset_of!(IMTLDXGIOutputVtbl, get_gamma_ramp) == 29 * slot);
    assert!(size_of::<IMTLDXGIOutputVtbl>() == 30 * slot);
    assert!(offset_of!(IMTLSwapChainFactoryVtbl, create_swap_chain) == 3 * slot);
    assert!(size_of::<IMTLSwapChainFactoryVtbl>() == 4 * slot);
};

#[cfg(test)]
mod tests;
