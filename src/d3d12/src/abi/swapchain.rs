//! C-layout mirrors for DXGI swap-chain descriptors (`DXGI_SWAP_CHAIN_DESC1`, fullscreen desc).
//!
//! The structures, constants and IIDs `swapchain` speaks (`dxgi1_6.h`, `d3d12.h`). The shared
//! descriptors live in [`dxmt_util::ext`]; the swap-chain-specific structures are defined here.
//! Every structure's layout is asserted against clang's `-fdump-record-layouts-complete` across
//! all five targets.

use core::{ffi::c_void, mem::offset_of};

pub use dxmt_util::{
    com::{
        DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL,
        E_POINTER, Guid, Hresult, IID_IUNKNOWN, S_FALSE, S_OK,
    },
    ext::{
        AdapterVtbl, Hmonitor, Hwnd, ModeDesc, ModeDesc1, OutputDesc, OutputDesc1, OutputVtbl,
        Rational, Rect, Rgb, SampleDesc, SwapChainDesc, SwapChainDesc1, SwapChainFullscreenDesc,
        Wchar,
    },
};

/// `DXGI_STATUS_OCCLUDED`.
pub const DXGI_STATUS_OCCLUDED: Hresult = 0x087a_0001;
/// `DXGI_STATUS_MODE_CHANGE_IN_PROGRESS`.
pub const DXGI_STATUS_MODE_CHANGE_IN_PROGRESS: Hresult = 0x087a_0025;
/// `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`.
pub const DXGI_ERROR_NOT_CURRENTLY_AVAILABLE: Hresult = 0x887a_0022_u32.cast_signed();

/// `DXGI_MAX_SWAP_CHAIN_BUFFERS`.
pub const DXGI_MAX_SWAP_CHAIN_BUFFERS: u32 = 16;
/// `DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH`.
pub const DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH: u32 = 2;
/// `DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT`.
pub const DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT: u32 = 64;

/// `DXGI_PRESENT_TEST`.
pub const DXGI_PRESENT_TEST: u32 = 1;

/// `DXGI_USAGE_UNORDERED_ACCESS` (`1L << (6 + 4)`).
pub const DXGI_USAGE_UNORDERED_ACCESS: u32 = 1 << (6 + 4);

/// `DXGI_FORMAT_R16G16B16A16_FLOAT`.
pub const DXGI_FORMAT_R16G16B16A16_FLOAT: u32 = 10;

/// `DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED`.
pub const DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED: u32 = 0;
/// `DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST`.
pub const DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST: u32 = 2;
/// `DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST`.
pub const DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST: u32 = 3;
/// `DXGI_MODE_SCALING_UNSPECIFIED`.
pub const DXGI_MODE_SCALING_UNSPECIFIED: u32 = 0;

/// `DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709`.
pub const DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709: u32 = 0;
/// `DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709`.
pub const DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709: u32 = 1;
/// `DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT`.
pub const DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT: u32 = 1;

/// `DXGI_HDR_METADATA_TYPE_NONE`.
pub const DXGI_HDR_METADATA_TYPE_NONE: u32 = 0;
/// `DXGI_HDR_METADATA_TYPE_HDR10`.
pub const DXGI_HDR_METADATA_TYPE_HDR10: u32 = 1;

/// `D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES`.
pub const D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES: u32 = 0x84;

/// `IDXGIAdapter` (`2411e7e1-12ac-4ccf-bd14-9798e8534dc0`).
pub const IID_IDXGI_ADAPTER: Guid = Guid::new(
    0x2411_e7e1,
    0x12ac,
    0x4ccf,
    [0xbd, 0x14, 0x97, 0x98, 0xe8, 0x53, 0x4d, 0xc0],
);
/// `IDXGIOutput` (`ae02eedb-c735-4690-8d52-5a8dc20213aa`).
pub const IID_IDXGI_OUTPUT: Guid = Guid::new(
    0xae02_eedb,
    0xc735,
    0x4690,
    [0x8d, 0x52, 0x5a, 0x8d, 0xc2, 0x02, 0x13, 0xaa],
);
/// `IDXGIOutput1` (`00cddea8-939b-4b83-a340-a685226666cc`).
pub const IID_IDXGI_OUTPUT1: Guid = Guid::new(
    0x00cd_dea8,
    0x939b,
    0x4b83,
    [0xa3, 0x40, 0xa6, 0x85, 0x22, 0x66, 0x66, 0xcc],
);

/// `DXGI_FRAME_STATISTICS`.
#[derive(Default)]
#[repr(C)]
pub struct FrameStatistics {
    pub present_count: u32,
    pub present_refresh_count: u32,
    pub sync_refresh_count: u32,
    pub sync_qpc_time: i64,
    pub sync_gpu_time: i64,
}

/// `DXGI_PRESENT_PARAMETERS`.
#[repr(C)]
pub struct PresentParameters {
    pub dirty_rects_count: u32,
    pub dirty_rects: *mut Rect,
    pub scroll_rect: *mut Rect,
    pub scroll_offset: *mut c_void,
}

/// `DXGI_MATRIX_3X2_F`.
#[derive(Default)]
#[repr(C)]
pub struct Matrix3x2f {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub m31: f32,
    pub m32: f32,
}

/// `DXGI_RGBA`.
#[derive(Default)]
#[repr(C)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

// The layouts of clang's `-fdump-record-layouts-complete` over `dxgi1_6.h`: every mirrored
// structure's size, alignment, and field offsets across all five targets.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(size_of::<FrameStatistics>() == 32 && align_of::<FrameStatistics>() == 8);
    assert!(offset_of!(FrameStatistics, present_count) == 0);
    assert!(offset_of!(FrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(FrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(FrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(FrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<Matrix3x2f>() == 24 && align_of::<Matrix3x2f>() == 4);
    assert!(offset_of!(Matrix3x2f, m11) == 0);
    assert!(offset_of!(Matrix3x2f, m12) == 4);
    assert!(offset_of!(Matrix3x2f, m21) == 8);
    assert!(offset_of!(Matrix3x2f, m22) == 12);
    assert!(offset_of!(Matrix3x2f, m31) == 16);
    assert!(offset_of!(Matrix3x2f, m32) == 20);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 160 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 128);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 144);
    assert!(offset_of!(OutputDesc, rotation) == 148);
    assert!(offset_of!(OutputDesc, monitor) == 152);
    assert!(size_of::<PresentParameters>() == 32 && align_of::<PresentParameters>() == 8);
    assert!(offset_of!(PresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(PresentParameters, dirty_rects) == 8);
    assert!(offset_of!(PresentParameters, scroll_rect) == 16);
    assert!(offset_of!(PresentParameters, scroll_offset) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgba>() == 16 && align_of::<Rgba>() == 4);
    assert!(offset_of!(Rgba, r) == 0);
    assert!(offset_of!(Rgba, g) == 4);
    assert!(offset_of!(Rgba, b) == 8);
    assert!(offset_of!(Rgba, a) == 12);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<FrameStatistics>() == 32 && align_of::<FrameStatistics>() == 8);
    assert!(offset_of!(FrameStatistics, present_count) == 0);
    assert!(offset_of!(FrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(FrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(FrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(FrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<Matrix3x2f>() == 24 && align_of::<Matrix3x2f>() == 4);
    assert!(offset_of!(Matrix3x2f, m11) == 0);
    assert!(offset_of!(Matrix3x2f, m12) == 4);
    assert!(offset_of!(Matrix3x2f, m21) == 8);
    assert!(offset_of!(Matrix3x2f, m22) == 12);
    assert!(offset_of!(Matrix3x2f, m31) == 16);
    assert!(offset_of!(Matrix3x2f, m32) == 20);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 96 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<PresentParameters>() == 32 && align_of::<PresentParameters>() == 8);
    assert!(offset_of!(PresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(PresentParameters, dirty_rects) == 8);
    assert!(offset_of!(PresentParameters, scroll_rect) == 16);
    assert!(offset_of!(PresentParameters, scroll_offset) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgba>() == 16 && align_of::<Rgba>() == 4);
    assert!(offset_of!(Rgba, r) == 0);
    assert!(offset_of!(Rgba, g) == 4);
    assert!(offset_of!(Rgba, b) == 8);
    assert!(offset_of!(Rgba, a) == 12);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: () = {
    assert!(size_of::<FrameStatistics>() == 32 && align_of::<FrameStatistics>() == 8);
    assert!(offset_of!(FrameStatistics, present_count) == 0);
    assert!(offset_of!(FrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(FrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(FrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(FrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<Matrix3x2f>() == 24 && align_of::<Matrix3x2f>() == 4);
    assert!(offset_of!(Matrix3x2f, m11) == 0);
    assert!(offset_of!(Matrix3x2f, m12) == 4);
    assert!(offset_of!(Matrix3x2f, m21) == 8);
    assert!(offset_of!(Matrix3x2f, m22) == 12);
    assert!(offset_of!(Matrix3x2f, m31) == 16);
    assert!(offset_of!(Matrix3x2f, m32) == 20);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 96 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<PresentParameters>() == 32 && align_of::<PresentParameters>() == 8);
    assert!(offset_of!(PresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(PresentParameters, dirty_rects) == 8);
    assert!(offset_of!(PresentParameters, scroll_rect) == 16);
    assert!(offset_of!(PresentParameters, scroll_offset) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgba>() == 16 && align_of::<Rgba>() == 4);
    assert!(offset_of!(Rgba, r) == 0);
    assert!(offset_of!(Rgba, g) == 4);
    assert!(offset_of!(Rgba, b) == 8);
    assert!(offset_of!(Rgba, a) == 12);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<FrameStatistics>() == 32 && align_of::<FrameStatistics>() == 8);
    assert!(offset_of!(FrameStatistics, present_count) == 0);
    assert!(offset_of!(FrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(FrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(FrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(FrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<Matrix3x2f>() == 24 && align_of::<Matrix3x2f>() == 4);
    assert!(offset_of!(Matrix3x2f, m11) == 0);
    assert!(offset_of!(Matrix3x2f, m12) == 4);
    assert!(offset_of!(Matrix3x2f, m21) == 8);
    assert!(offset_of!(Matrix3x2f, m22) == 12);
    assert!(offset_of!(Matrix3x2f, m31) == 16);
    assert!(offset_of!(Matrix3x2f, m32) == 20);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 96 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<PresentParameters>() == 32 && align_of::<PresentParameters>() == 8);
    assert!(offset_of!(PresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(PresentParameters, dirty_rects) == 8);
    assert!(offset_of!(PresentParameters, scroll_rect) == 16);
    assert!(offset_of!(PresentParameters, scroll_offset) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgba>() == 16 && align_of::<Rgba>() == 4);
    assert!(offset_of!(Rgba, r) == 0);
    assert!(offset_of!(Rgba, g) == 4);
    assert!(offset_of!(Rgba, b) == 8);
    assert!(offset_of!(Rgba, a) == 12);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(size_of::<FrameStatistics>() == 32 && align_of::<FrameStatistics>() == 8);
    assert!(offset_of!(FrameStatistics, present_count) == 0);
    assert!(offset_of!(FrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(FrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(FrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(FrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<Matrix3x2f>() == 24 && align_of::<Matrix3x2f>() == 4);
    assert!(offset_of!(Matrix3x2f, m11) == 0);
    assert!(offset_of!(Matrix3x2f, m12) == 4);
    assert!(offset_of!(Matrix3x2f, m21) == 8);
    assert!(offset_of!(Matrix3x2f, m22) == 12);
    assert!(offset_of!(Matrix3x2f, m31) == 16);
    assert!(offset_of!(Matrix3x2f, m32) == 20);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 92 && align_of::<OutputDesc>() == 4);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<PresentParameters>() == 16 && align_of::<PresentParameters>() == 4);
    assert!(offset_of!(PresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(PresentParameters, dirty_rects) == 4);
    assert!(offset_of!(PresentParameters, scroll_rect) == 8);
    assert!(offset_of!(PresentParameters, scroll_offset) == 12);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgba>() == 16 && align_of::<Rgba>() == 4);
    assert!(offset_of!(Rgba, r) == 0);
    assert!(offset_of!(Rgba, g) == 4);
    assert!(offset_of!(Rgba, b) == 8);
    assert!(offset_of!(Rgba, a) == 12);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 60 && align_of::<SwapChainDesc>() == 4);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 44);
    assert!(offset_of!(SwapChainDesc, windowed) == 48);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 52);
    assert!(offset_of!(SwapChainDesc, flags) == 56);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};
