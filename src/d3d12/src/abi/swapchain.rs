//! C-layout mirrors for DXGI swap-chain descriptors (`DXGI_SWAP_CHAIN_DESC1`, fullscreen desc).
//!
//! The structures, constants and IIDs `swapchain` speaks (`dxgi1_6.h`, `d3d12.h`). The shared
//! descriptors live in [`dxmt_util::ext`]; the swap-chain-specific structures are defined here.
//! Every structure's layout is asserted against clang's `-fdump-record-layouts-complete` across
//! all five targets.

use core::{ffi::c_void, mem::offset_of};

pub use dxmt_util::{
    com::{
        Guid, Hresult,
        hresult::{
            DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND,
            DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, DXGI_STATUS_OCCLUDED, E_FAIL, E_INVALIDARG,
            E_NOINTERFACE, E_NOTIMPL, E_POINTER, S_FALSE, S_OK,
        },
        iid::{IID_IDXGI_ADAPTER, IID_IDXGI_OUTPUT, IID_IDXGI_OUTPUT1, IID_IUNKNOWN},
    },
    dxgi_consts::{
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_HDR_METADATA_TYPE_HDR10, DXGI_HDR_METADATA_TYPE_NONE,
        DXGI_MAX_SWAP_CHAIN_BUFFERS, DXGI_MODE_SCALING_UNSPECIFIED,
        DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST, DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
        DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST, DXGI_PRESENT_TEST,
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT, DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH,
        DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_USAGE_UNORDERED_ACCESS,
    },
    ext::{
        AdapterVtbl, FrameStatistics, Hmonitor, Hwnd, ModeDesc, ModeDesc1, OutputDesc, OutputDesc1,
        OutputVtbl, Rational, Rect, Rgb, SampleDesc, SwapChainDesc, SwapChainDesc1,
        SwapChainFullscreenDesc, Wchar,
    },
};

/// `D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES`.
pub const D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES: u32 = 0x84;

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
