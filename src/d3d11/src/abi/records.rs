//! `d3d11_4.h+d3d10_1.h`'s C records (`crate::abi::records`).
//!
//! Written by `tools/winemetal-layouts.sh --com` from clang's AST on each target the crates
//! build for: regenerate it, never edit it.

use core::{ffi::{c_char, c_void}, mem::offset_of};

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

/// `POINT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// `SECURITY_ATTRIBUTES`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SecurityAttributes {
    pub n_length: u32,
    pub lp_security_descriptor: *mut c_void,
    pub b_inherit_handle: i32,
}

/// `DXGI_RATIONAL`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiRational {
    pub numerator: u32,
    pub denominator: u32,
}

/// `DXGI_SAMPLE_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiSampleDesc {
    pub count: u32,
    pub quality: u32,
}

/// `DXGI_RGB`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiRgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

/// `DXGI_RGBA`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// `DXGI_GAMMA_CONTROL`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiGammaControl {
    pub scale: DxgiRgb,
    pub offset: DxgiRgb,
    pub gamma_curve: [DxgiRgb; 1025],
}

/// `DXGI_GAMMA_CONTROL_CAPABILITIES`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiGammaControlCapabilities {
    pub scale_and_offset_supported: i32,
    pub max_converted_value: f32,
    pub min_converted_value: f32,
    pub num_gamma_control_points: u32,
    pub control_point_positions: [f32; 1025],
}

/// `DXGI_MODE_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiModeDesc {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: DxgiRational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
}

/// `DXGI_MODE_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiModeDesc1 {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: DxgiRational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub stereo: i32,
}

/// `DXGI_SURFACE_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiSurfaceDesc {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub sample_desc: DxgiSampleDesc,
}

/// `DXGI_MAPPED_RECT`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiMappedRect {
    pub pitch: i32,
    pub p_bits: *mut u8,
}

/// `DXGI_SHARED_RESOURCE`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiSharedResource {
    pub handle: *mut c_void,
}

/// `DXGI_FRAME_STATISTICS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiFrameStatistics {
    pub present_count: u32,
    pub present_refresh_count: u32,
    pub sync_refresh_count: u32,
    pub sync_qpc_time: i64,
    pub sync_gpu_time: i64,
}

/// `DXGI_SWAP_CHAIN_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiSwapChainDesc {
    pub buffer_desc: DxgiModeDesc,
    pub sample_desc: DxgiSampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub output_window: *mut c_void,
    pub windowed: i32,
    pub swap_effect: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiSwapChainDesc1 {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub stereo: i32,
    pub sample_desc: DxgiSampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub scaling: u32,
    pub swap_effect: u32,
    pub alpha_mode: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_FULLSCREEN_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiSwapChainFullscreenDesc {
    pub refresh_rate: DxgiRational,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub windowed: i32,
}

/// `DXGI_PRESENT_PARAMETERS`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DxgiPresentParameters {
    pub dirty_rects_count: u32,
    pub p_dirty_rects: *mut Rect,
    pub p_scroll_rect: *mut Rect,
    pub p_scroll_offset: *mut Point,
}

/// `DXGI_MATRIX_3X2_F`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiMatrix3x2F {
    pub m_11: f32,
    pub m_12: f32,
    pub m_21: f32,
    pub m_22: f32,
    pub m_31: f32,
    pub m_32: f32,
}

/// `DXGI_HDR_METADATA_HDR10`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DxgiHdrMetadataHdr10 {
    pub red_primary: [u16; 2],
    pub green_primary: [u16; 2],
    pub blue_primary: [u16; 2],
    pub white_point: [u16; 2],
    pub max_mastering_luminance: u32,
    pub min_mastering_luminance: u32,
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}

/// `D3D11_BOX`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Box {
    pub left: u32,
    pub top: u32,
    pub front: u32,
    pub right: u32,
    pub bottom: u32,
    pub back: u32,
}

/// `D3D11_VIEWPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Viewport {
    pub top_left_x: f32,
    pub top_left_y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

/// `D3D11_MAPPED_SUBRESOURCE`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11MappedSubresource {
    pub p_data: *mut c_void,
    pub row_pitch: u32,
    pub depth_pitch: u32,
}

/// `D3D11_SUBRESOURCE_DATA`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11SubresourceData {
    pub p_sys_mem: *const c_void,
    pub sys_mem_pitch: u32,
    pub sys_mem_slice_pitch: u32,
}

/// `D3D11_BUFFER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BufferDesc {
    pub byte_width: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
    pub structure_byte_stride: u32,
}

/// `D3D11_TEXTURE1D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Texture1dDesc {
    pub width: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D11_TEXTURE2D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Texture2dDesc {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub sample_desc: DxgiSampleDesc,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D11_TEXTURE2D_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Texture2dDesc1 {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub sample_desc: DxgiSampleDesc,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
    pub texture_layout: u32,
}

/// `D3D11_TEXTURE3D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Texture3dDesc {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub format: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D11_TEXTURE3D_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Texture3dDesc1 {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub format: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
    pub texture_layout: u32,
}

/// `D3D11_BUFFER_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BufferSrv {
    pub first_element: u32,
    pub num_elements: u32,
}

/// `D3D11_BUFFEREX_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BufferexSrv {
    pub first_element: u32,
    pub num_elements: u32,
    pub flags: u32,
}

/// `D3D11_TEX1D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D11_TEX1D_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D11_TEX2D_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_SRV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dSrv1 {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_SRV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArraySrv1 {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX3D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex3dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D11_TEXCUBE_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11TexcubeSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D11_TEXCUBE_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11TexcubeArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first2d_array_face: u32,
    pub num_cubes: u32,
}

/// `D3D11_TEX2DMS_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsSrv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D11_TEX2DMS_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsArraySrv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_SHADER_RESOURCE_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11ShaderResourceViewDescUnion {
    pub buffer: D3d11BufferSrv,
    pub texture1d: D3d11Tex1dSrv,
    pub texture1d_array: D3d11Tex1dArraySrv,
    pub texture2d: D3d11Tex2dSrv,
    pub texture2d_array: D3d11Tex2dArraySrv,
    pub texture2dms: D3d11Tex2dmsSrv,
    pub texture2dms_array: D3d11Tex2dmsArraySrv,
    pub texture3d: D3d11Tex3dSrv,
    pub texture_cube: D3d11TexcubeSrv,
    pub texture_cube_array: D3d11TexcubeArraySrv,
    pub buffer_ex: D3d11BufferexSrv,
}

/// `D3D11_SHADER_RESOURCE_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11ShaderResourceViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11ShaderResourceViewDescUnion,
}

/// `D3D11_SHADER_RESOURCE_VIEW_DESC1`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11ShaderResourceViewDesc1Union {
    pub buffer: D3d11BufferSrv,
    pub texture1d: D3d11Tex1dSrv,
    pub texture1d_array: D3d11Tex1dArraySrv,
    pub texture2d: D3d11Tex2dSrv1,
    pub texture2d_array: D3d11Tex2dArraySrv1,
    pub texture2dms: D3d11Tex2dmsSrv,
    pub texture2dms_array: D3d11Tex2dmsArraySrv,
    pub texture3d: D3d11Tex3dSrv,
    pub texture_cube: D3d11TexcubeSrv,
    pub texture_cube_array: D3d11TexcubeArraySrv,
    pub buffer_ex: D3d11BufferexSrv,
}

/// `D3D11_SHADER_RESOURCE_VIEW_DESC1`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11ShaderResourceViewDesc1 {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11ShaderResourceViewDesc1Union,
}

/// `D3D11_BUFFER_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BufferRtv {
    pub first_element: u32,
    pub num_elements: u32,
}

/// `D3D11_TEX1D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dRtv {
    pub mip_slice: u32,
}

/// `D3D11_TEX1D_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dRtv {
    pub mip_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_RTV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dRtv1 {
    pub mip_slice: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_RTV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArrayRtv1 {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX2DMS_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsRtv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D11_TEX2DMS_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsArrayRtv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX3D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex3dRtv {
    pub mip_slice: u32,
    pub first_w_slice: u32,
    pub w_size: u32,
}

/// `D3D11_RENDER_TARGET_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11RenderTargetViewDescUnion {
    pub buffer: D3d11BufferRtv,
    pub texture1d: D3d11Tex1dRtv,
    pub texture1d_array: D3d11Tex1dArrayRtv,
    pub texture2d: D3d11Tex2dRtv,
    pub texture2d_array: D3d11Tex2dArrayRtv,
    pub texture2dms: D3d11Tex2dmsRtv,
    pub texture2dms_array: D3d11Tex2dmsArrayRtv,
    pub texture3d: D3d11Tex3dRtv,
}

/// `D3D11_RENDER_TARGET_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11RenderTargetViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11RenderTargetViewDescUnion,
}

/// `D3D11_RENDER_TARGET_VIEW_DESC1`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11RenderTargetViewDesc1Union {
    pub buffer: D3d11BufferRtv,
    pub texture1d: D3d11Tex1dRtv,
    pub texture1d_array: D3d11Tex1dArrayRtv,
    pub texture2d: D3d11Tex2dRtv1,
    pub texture2d_array: D3d11Tex2dArrayRtv1,
    pub texture2dms: D3d11Tex2dmsRtv,
    pub texture2dms_array: D3d11Tex2dmsArrayRtv,
    pub texture3d: D3d11Tex3dRtv,
}

/// `D3D11_RENDER_TARGET_VIEW_DESC1`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11RenderTargetViewDesc1 {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11RenderTargetViewDesc1Union,
}

/// `D3D11_TEX1D_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dDsv {
    pub mip_slice: u32,
}

/// `D3D11_TEX1D_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dDsv {
    pub mip_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2DMS_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsDsv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D11_TEX2DMS_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dmsArrayDsv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_DEPTH_STENCIL_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11DepthStencilViewDescUnion {
    pub texture1d: D3d11Tex1dDsv,
    pub texture1d_array: D3d11Tex1dArrayDsv,
    pub texture2d: D3d11Tex2dDsv,
    pub texture2d_array: D3d11Tex2dArrayDsv,
    pub texture2dms: D3d11Tex2dmsDsv,
    pub texture2dms_array: D3d11Tex2dmsArrayDsv,
}

/// `D3D11_DEPTH_STENCIL_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11DepthStencilViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub flags: u32,
    pub u: D3d11DepthStencilViewDescUnion,
}

/// `D3D11_BUFFER_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BufferUav {
    pub first_element: u32,
    pub num_elements: u32,
    pub flags: u32,
}

/// `D3D11_TEX1D_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dUav {
    pub mip_slice: u32,
}

/// `D3D11_TEX1D_ARRAY_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex1dArrayUav {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dUav {
    pub mip_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArrayUav {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D11_TEX2D_UAV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dUav1 {
    pub mip_slice: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX2D_ARRAY_UAV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex2dArrayUav1 {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
}

/// `D3D11_TEX3D_UAV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11Tex3dUav {
    pub mip_slice: u32,
    pub first_w_slice: u32,
    pub w_size: u32,
}

/// `D3D11_UNORDERED_ACCESS_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11UnorderedAccessViewDescUnion {
    pub buffer: D3d11BufferUav,
    pub texture1d: D3d11Tex1dUav,
    pub texture1d_array: D3d11Tex1dArrayUav,
    pub texture2d: D3d11Tex2dUav,
    pub texture2d_array: D3d11Tex2dArrayUav,
    pub texture3d: D3d11Tex3dUav,
}

/// `D3D11_UNORDERED_ACCESS_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11UnorderedAccessViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11UnorderedAccessViewDescUnion,
}

/// `D3D11_UNORDERED_ACCESS_VIEW_DESC1`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d11UnorderedAccessViewDesc1Union {
    pub buffer: D3d11BufferUav,
    pub texture1d: D3d11Tex1dUav,
    pub texture1d_array: D3d11Tex1dArrayUav,
    pub texture2d: D3d11Tex2dUav1,
    pub texture2d_array: D3d11Tex2dArrayUav1,
    pub texture3d: D3d11Tex3dUav,
}

/// `D3D11_UNORDERED_ACCESS_VIEW_DESC1`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11UnorderedAccessViewDesc1 {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d11UnorderedAccessViewDesc1Union,
}

/// `D3D11_RENDER_TARGET_BLEND_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11RenderTargetBlendDesc {
    pub blend_enable: i32,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub render_target_write_mask: u8,
}

/// `D3D11_BLEND_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BlendDesc {
    pub alpha_to_coverage_enable: i32,
    pub independent_blend_enable: i32,
    pub render_target: [D3d11RenderTargetBlendDesc; 8],
}

/// `D3D11_RENDER_TARGET_BLEND_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11RenderTargetBlendDesc1 {
    pub blend_enable: i32,
    pub logic_op_enable: i32,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub logic_op: u32,
    pub render_target_write_mask: u8,
}

/// `D3D11_BLEND_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11BlendDesc1 {
    pub alpha_to_coverage_enable: i32,
    pub independent_blend_enable: i32,
    pub render_target: [D3d11RenderTargetBlendDesc1; 8],
}

/// `D3D11_DEPTH_STENCILOP_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11DepthStencilopDesc {
    pub stencil_fail_op: u32,
    pub stencil_depth_fail_op: u32,
    pub stencil_pass_op: u32,
    pub stencil_func: u32,
}

/// `D3D11_DEPTH_STENCIL_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11DepthStencilDesc {
    pub depth_enable: i32,
    pub depth_write_mask: u32,
    pub depth_func: u32,
    pub stencil_enable: i32,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: D3d11DepthStencilopDesc,
    pub back_face: D3d11DepthStencilopDesc,
}

/// `D3D11_RASTERIZER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11RasterizerDesc {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub scissor_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
}

/// `D3D11_RASTERIZER_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11RasterizerDesc1 {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub scissor_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
    pub forced_sample_count: u32,
}

/// `D3D11_RASTERIZER_DESC2`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11RasterizerDesc2 {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub scissor_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
    pub forced_sample_count: u32,
    pub conservative_raster: u32,
}

/// `D3D11_SAMPLER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11SamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: [f32; 4],
    pub min_lod: f32,
    pub max_lod: f32,
}

/// `D3D11_INPUT_ELEMENT_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11InputElementDesc {
    pub semantic_name: *const c_char,
    pub semantic_index: u32,
    pub format: u32,
    pub input_slot: u32,
    pub aligned_byte_offset: u32,
    pub input_slot_class: u32,
    pub instance_data_step_rate: u32,
}

/// `D3D11_SO_DECLARATION_ENTRY`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d11SoDeclarationEntry {
    pub stream: u32,
    pub semantic_name: *const c_char,
    pub semantic_index: u32,
    pub start_component: u8,
    pub component_count: u8,
    pub output_slot: u8,
}

/// `D3D11_CLASS_INSTANCE_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11ClassInstanceDesc {
    pub instance_id: u32,
    pub instance_index: u32,
    pub type_id: u32,
    pub constant_buffer: u32,
    pub base_constant_buffer_offset: u32,
    pub base_texture: u32,
    pub base_sampler: u32,
    pub created: i32,
}

/// `D3D11_QUERY_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11QueryDesc {
    pub query: u32,
    pub misc_flags: u32,
}

/// `D3D11_QUERY_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11QueryDesc1 {
    pub query: u32,
    pub misc_flags: u32,
    pub context_type: u32,
}

/// `D3D11_QUERY_DATA_TIMESTAMP_DISJOINT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11QueryDataTimestampDisjoint {
    pub frequency: u64,
    pub disjoint: i32,
}

/// `D3D11_QUERY_DATA_PIPELINE_STATISTICS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11QueryDataPipelineStatistics {
    pub ia_vertices: u64,
    pub ia_primitives: u64,
    pub vs_invocations: u64,
    pub gs_invocations: u64,
    pub gs_primitives: u64,
    pub c_invocations: u64,
    pub c_primitives: u64,
    pub ps_invocations: u64,
    pub hs_invocations: u64,
    pub ds_invocations: u64,
    pub cs_invocations: u64,
}

/// `D3D11_QUERY_DATA_SO_STATISTICS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11QueryDataSoStatistics {
    pub num_primitives_written: u64,
    pub primitives_storage_needed: u64,
}

/// `D3D11_COUNTER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11CounterDesc {
    pub counter: u32,
    pub misc_flags: u32,
}

/// `D3D11_COUNTER_INFO`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11CounterInfo {
    pub last_device_dependent_counter: u32,
    pub num_simultaneous_counters: u32,
    pub num_detectable_parallel_units: u8,
}

/// `D3D11_TILED_RESOURCE_COORDINATE`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11TiledResourceCoordinate {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub subresource: u32,
}

/// `D3D11_TILE_REGION_SIZE`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11TileRegionSize {
    pub num_tiles: u32,
    pub b_use_box: i32,
    pub width: u32,
    pub height: u16,
    pub depth: u16,
}

/// `D3D11_TILE_SHAPE`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11TileShape {
    pub width_in_texels: u32,
    pub height_in_texels: u32,
    pub depth_in_texels: u32,
}

/// `D3D11_SUBRESOURCE_TILING`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11SubresourceTiling {
    pub width_in_tiles: u32,
    pub height_in_tiles: u16,
    pub depth_in_tiles: u16,
    pub start_tile_index_in_overall_resource: u32,
}

/// `D3D11_PACKED_MIP_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11PackedMipDesc {
    pub num_standard_mips: u8,
    pub num_packed_mips: u8,
    pub num_tiles_for_packed_mips: u32,
    pub start_tile_index_in_overall_resource: u32,
}

/// `D3D11_DRAW_INSTANCED_INDIRECT_ARGS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11DrawInstancedIndirectArgs {
    pub vertex_count_per_instance: u32,
    pub instance_count: u32,
    pub start_vertex_location: u32,
    pub start_instance_location: u32,
}

/// `D3D11_DRAW_INDEXED_INSTANCED_INDIRECT_ARGS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11DrawIndexedInstancedIndirectArgs {
    pub index_count_per_instance: u32,
    pub instance_count: u32,
    pub start_index_location: u32,
    pub base_vertex_location: i32,
    pub start_instance_location: u32,
}

/// `D3D11_FEATURE_DATA_THREADING`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataThreading {
    pub driver_concurrent_creates: i32,
    pub driver_command_lists: i32,
}

/// `D3D11_FEATURE_DATA_DOUBLES`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataDoubles {
    pub double_precision_float_shader_ops: i32,
}

/// `D3D11_FEATURE_DATA_FORMAT_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataFormatSupport {
    pub in_format: u32,
    pub out_format_support: u32,
}

/// `D3D11_FEATURE_DATA_FORMAT_SUPPORT2`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataFormatSupport2 {
    pub in_format: u32,
    pub out_format_support2: u32,
}

/// `D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d10XHardwareOptions {
    pub compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x: i32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options {
    pub output_merger_logic_op: i32,
    pub uav_only_rendering_forced_sample_count: i32,
    pub discard_ap_is_seen_by_driver: i32,
    pub flags_for_update_and_copy_seen_by_driver: i32,
    pub clear_view: i32,
    pub copy_with_overlap: i32,
    pub constant_buffer_partial_update: i32,
    pub constant_buffer_offsetting: i32,
    pub map_no_overwrite_on_dynamic_constant_buffer: i32,
    pub map_no_overwrite_on_dynamic_buffer_srv: i32,
    pub multisample_rtv_with_forced_sample_count_one: i32,
    pub sad4_shader_instructions: i32,
    pub extended_doubles_shader_instructions: i32,
    pub extended_resource_sharing: i32,
}

/// `D3D11_FEATURE_DATA_ARCHITECTURE_INFO`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataArchitectureInfo {
    pub tile_based_deferred_renderer: i32,
}

/// `D3D11_FEATURE_DATA_D3D9_OPTIONS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d9Options {
    pub full_non_pow2_texture_support: i32,
}

/// `D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d9ShadowSupport {
    pub supports_depth_as_texture_with_less_equal_comparison_filter: i32,
}

/// `D3D11_FEATURE_DATA_SHADER_MIN_PRECISION_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataShaderMinPrecisionSupport {
    pub pixel_shader_min_precision: u32,
    pub all_other_shader_stages_min_precision: u32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options1 {
    pub tiled_resources_tier: u32,
    pub min_max_filtering: i32,
    pub clear_view_also_supports_depth_only_formats: i32,
    pub map_on_default_buffers: i32,
}

/// `D3D11_FEATURE_DATA_D3D9_SIMPLE_INSTANCING_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d9SimpleInstancingSupport {
    pub simple_instancing_supported: i32,
}

/// `D3D11_FEATURE_DATA_MARKER_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataMarkerSupport {
    pub profile: i32,
}

/// `D3D11_FEATURE_DATA_D3D9_OPTIONS1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d9Options1 {
    pub full_non_pow2_texture_supported: i32,
    pub depth_as_texture_with_less_equal_comparison_filter_supported: i32,
    pub simple_instancing_supported: i32,
    pub texture_cube_face_render_target_with_non_cube_depth_stencil_supported: i32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS2`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options2 {
    pub ps_specified_stencil_ref_supported: i32,
    pub typed_uav_load_additional_formats: i32,
    pub ro_vs_supported: i32,
    pub conservative_rasterization_tier: u32,
    pub tiled_resources_tier: u32,
    pub map_on_default_textures: i32,
    pub standard_swizzle: i32,
    pub unified_memory_architecture: i32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS3`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options3 {
    pub vp_and_rt_array_index_from_any_shader_feeding_rasterizer: i32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS4`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options4 {
    pub extended_nv12_shared_texture_supported: i32,
}

/// `D3D11_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataGpuVirtualAddressSupport {
    pub max_gpu_virtual_address_bits_per_resource: u32,
    pub max_gpu_virtual_address_bits_per_process: u32,
}

/// `D3D11_FEATURE_DATA_D3D11_OPTIONS5`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataD3d11Options5 {
    pub shared_resource_tier: u32,
}

/// `D3D11_FEATURE_DATA_SHADER_CACHE`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d11FeatureDataShaderCache {
    pub support_flags: u32,
}

/// `D3D10_BOX`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Box {
    pub left: u32,
    pub top: u32,
    pub front: u32,
    pub right: u32,
    pub bottom: u32,
    pub back: u32,
}

/// `D3D10_VIEWPORT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Viewport {
    pub top_left_x: i32,
    pub top_left_y: i32,
    pub width: u32,
    pub height: u32,
    pub min_depth: f32,
    pub max_depth: f32,
}

/// `D3D10_MAPPED_TEXTURE2D`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10MappedTexture2d {
    pub p_data: *mut c_void,
    pub row_pitch: u32,
}

/// `D3D10_MAPPED_TEXTURE3D`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10MappedTexture3d {
    pub p_data: *mut c_void,
    pub row_pitch: u32,
    pub depth_pitch: u32,
}

/// `D3D10_SUBRESOURCE_DATA`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10SubresourceData {
    pub p_sys_mem: *const c_void,
    pub sys_mem_pitch: u32,
    pub sys_mem_slice_pitch: u32,
}

/// `D3D10_BUFFER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10BufferDesc {
    pub byte_width: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D10_TEXTURE1D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Texture1dDesc {
    pub width: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D10_TEXTURE2D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Texture2dDesc {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub sample_desc: DxgiSampleDesc,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D10_TEXTURE3D_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Texture3dDesc {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub format: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

/// `D3D10_BUFFER_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10BufferSrv {
    pub first_element: u32,
    pub num_elements: u32,
}

/// `D3D10_TEX1D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D10_TEX1D_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX2D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D10_TEX2D_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX3D_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex3dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D10_TEXCUBE_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10TexcubeSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
}

/// `D3D10_TEXCUBE_ARRAY_SRV1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10TexcubeArraySrv1 {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first2d_array_face: u32,
    pub num_cubes: u32,
}

/// `D3D10_TEX2DMS_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsSrv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D10_TEX2DMS_ARRAY_SRV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsArraySrv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_SHADER_RESOURCE_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d10ShaderResourceViewDescUnion {
    pub buffer: D3d10BufferSrv,
    pub texture1d: D3d10Tex1dSrv,
    pub texture1d_array: D3d10Tex1dArraySrv,
    pub texture2d: D3d10Tex2dSrv,
    pub texture2d_array: D3d10Tex2dArraySrv,
    pub texture2dms: D3d10Tex2dmsSrv,
    pub texture2dms_array: D3d10Tex2dmsArraySrv,
    pub texture3d: D3d10Tex3dSrv,
    pub texture_cube: D3d10TexcubeSrv,
}

/// `D3D10_SHADER_RESOURCE_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10ShaderResourceViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d10ShaderResourceViewDescUnion,
}

/// `D3D10_SHADER_RESOURCE_VIEW_DESC1`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d10ShaderResourceViewDesc1Union {
    pub buffer: D3d10BufferSrv,
    pub texture1d: D3d10Tex1dSrv,
    pub texture1d_array: D3d10Tex1dArraySrv,
    pub texture2d: D3d10Tex2dSrv,
    pub texture2d_array: D3d10Tex2dArraySrv,
    pub texture2dms: D3d10Tex2dmsSrv,
    pub texture2dms_array: D3d10Tex2dmsArraySrv,
    pub texture3d: D3d10Tex3dSrv,
    pub texture_cube: D3d10TexcubeSrv,
    pub texture_cube_array: D3d10TexcubeArraySrv1,
}

/// `D3D10_SHADER_RESOURCE_VIEW_DESC1`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10ShaderResourceViewDesc1 {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d10ShaderResourceViewDesc1Union,
}

/// `D3D10_BUFFER_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10BufferRtv {
    pub first_element: u32,
    pub num_elements: u32,
}

/// `D3D10_TEX1D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dRtv {
    pub mip_slice: u32,
}

/// `D3D10_TEX1D_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX2D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dRtv {
    pub mip_slice: u32,
}

/// `D3D10_TEX2D_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX2DMS_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsRtv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D10_TEX2DMS_ARRAY_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsArrayRtv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX3D_RTV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex3dRtv {
    pub mip_slice: u32,
    pub first_w_slice: u32,
    pub w_size: u32,
}

/// `D3D10_RENDER_TARGET_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d10RenderTargetViewDescUnion {
    pub buffer: D3d10BufferRtv,
    pub texture1d: D3d10Tex1dRtv,
    pub texture1d_array: D3d10Tex1dArrayRtv,
    pub texture2d: D3d10Tex2dRtv,
    pub texture2d_array: D3d10Tex2dArrayRtv,
    pub texture2dms: D3d10Tex2dmsRtv,
    pub texture2dms_array: D3d10Tex2dmsArrayRtv,
    pub texture3d: D3d10Tex3dRtv,
}

/// `D3D10_RENDER_TARGET_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10RenderTargetViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d10RenderTargetViewDescUnion,
}

/// `D3D10_TEX1D_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dDsv {
    pub mip_slice: u32,
}

/// `D3D10_TEX1D_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex1dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX2D_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dDsv {
    pub mip_slice: u32,
}

/// `D3D10_TEX2D_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_TEX2DMS_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsDsv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D10_TEX2DMS_ARRAY_DSV`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10Tex2dmsArrayDsv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D10_DEPTH_STENCIL_VIEW_DESC`'s union.
#[derive(Clone, Copy)]
#[repr(C)]
pub union D3d10DepthStencilViewDescUnion {
    pub texture1d: D3d10Tex1dDsv,
    pub texture1d_array: D3d10Tex1dArrayDsv,
    pub texture2d: D3d10Tex2dDsv,
    pub texture2d_array: D3d10Tex2dArrayDsv,
    pub texture2dms: D3d10Tex2dmsDsv,
    pub texture2dms_array: D3d10Tex2dmsArrayDsv,
}

/// `D3D10_DEPTH_STENCIL_VIEW_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10DepthStencilViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub u: D3d10DepthStencilViewDescUnion,
}

/// `D3D10_BLEND_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10BlendDesc {
    pub alpha_to_coverage_enable: i32,
    pub blend_enable: [i32; 8],
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub render_target_write_mask: [u8; 8],
}

/// `D3D10_RENDER_TARGET_BLEND_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10RenderTargetBlendDesc1 {
    pub blend_enable: i32,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub render_target_write_mask: u8,
}

/// `D3D10_BLEND_DESC1`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10BlendDesc1 {
    pub alpha_to_coverage_enable: i32,
    pub independent_blend_enable: i32,
    pub render_target: [D3d10RenderTargetBlendDesc1; 8],
}

/// `D3D10_DEPTH_STENCILOP_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10DepthStencilopDesc {
    pub stencil_fail_op: u32,
    pub stencil_depth_fail_op: u32,
    pub stencil_pass_op: u32,
    pub stencil_func: u32,
}

/// `D3D10_DEPTH_STENCIL_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10DepthStencilDesc {
    pub depth_enable: i32,
    pub depth_write_mask: u32,
    pub depth_func: u32,
    pub stencil_enable: i32,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: D3d10DepthStencilopDesc,
    pub back_face: D3d10DepthStencilopDesc,
}

/// `D3D10_RASTERIZER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10RasterizerDesc {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub scissor_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
}

/// `D3D10_SAMPLER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10SamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: [f32; 4],
    pub min_lod: f32,
    pub max_lod: f32,
}

/// `D3D10_INPUT_ELEMENT_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10InputElementDesc {
    pub semantic_name: *const c_char,
    pub semantic_index: u32,
    pub format: u32,
    pub input_slot: u32,
    pub aligned_byte_offset: u32,
    pub input_slot_class: u32,
    pub instance_data_step_rate: u32,
}

/// `D3D10_SO_DECLARATION_ENTRY`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct D3d10SoDeclarationEntry {
    pub semantic_name: *const c_char,
    pub semantic_index: u32,
    pub start_component: u8,
    pub component_count: u8,
    pub output_slot: u8,
}

/// `D3D10_QUERY_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10QueryDesc {
    pub query: u32,
    pub misc_flags: u32,
}

/// `D3D10_QUERY_DATA_TIMESTAMP_DISJOINT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10QueryDataTimestampDisjoint {
    pub frequency: u64,
    pub disjoint: i32,
}

/// `D3D10_QUERY_DATA_PIPELINE_STATISTICS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10QueryDataPipelineStatistics {
    pub ia_vertices: u64,
    pub ia_primitives: u64,
    pub vs_invocations: u64,
    pub gs_invocations: u64,
    pub gs_primitives: u64,
    pub c_invocations: u64,
    pub c_primitives: u64,
    pub ps_invocations: u64,
}

/// `D3D10_QUERY_DATA_SO_STATISTICS`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10QueryDataSoStatistics {
    pub num_primitives_written: u64,
    pub primitives_storage_needed: u64,
}

/// `D3D10_COUNTER_DESC`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10CounterDesc {
    pub counter: u32,
    pub misc_flags: u32,
}

/// `D3D10_COUNTER_INFO`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct D3d10CounterInfo {
    pub last_device_dependent_counter: u32,
    pub num_simultaneous_counters: u32,
    pub num_detectable_parallel_units: u8,
}


#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<Luid>() == 8);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<Rect>() == 16);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Point>() == 8);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
    assert!(size_of::<SecurityAttributes>() == 24);
    assert!(offset_of!(SecurityAttributes, n_length) == 0);
    assert!(offset_of!(SecurityAttributes, lp_security_descriptor) == 8);
    assert!(offset_of!(SecurityAttributes, b_inherit_handle) == 16);
    assert!(size_of::<DxgiRational>() == 8);
    assert!(offset_of!(DxgiRational, numerator) == 0);
    assert!(offset_of!(DxgiRational, denominator) == 4);
    assert!(size_of::<DxgiSampleDesc>() == 8);
    assert!(offset_of!(DxgiSampleDesc, count) == 0);
    assert!(offset_of!(DxgiSampleDesc, quality) == 4);
    assert!(size_of::<DxgiRgb>() == 12);
    assert!(offset_of!(DxgiRgb, red) == 0);
    assert!(offset_of!(DxgiRgb, green) == 4);
    assert!(offset_of!(DxgiRgb, blue) == 8);
    assert!(size_of::<DxgiRgba>() == 16);
    assert!(offset_of!(DxgiRgba, r) == 0);
    assert!(offset_of!(DxgiRgba, g) == 4);
    assert!(offset_of!(DxgiRgba, b) == 8);
    assert!(offset_of!(DxgiRgba, a) == 12);
    assert!(size_of::<DxgiGammaControl>() == 12324);
    assert!(offset_of!(DxgiGammaControl, scale) == 0);
    assert!(offset_of!(DxgiGammaControl, offset) == 12);
    assert!(offset_of!(DxgiGammaControl, gamma_curve) == 24);
    assert!(size_of::<DxgiGammaControlCapabilities>() == 4116);
    assert!(offset_of!(DxgiGammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(DxgiGammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(DxgiGammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(DxgiGammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(DxgiGammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<DxgiModeDesc>() == 28);
    assert!(offset_of!(DxgiModeDesc, width) == 0);
    assert!(offset_of!(DxgiModeDesc, height) == 4);
    assert!(offset_of!(DxgiModeDesc, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc, format) == 16);
    assert!(offset_of!(DxgiModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc, scaling) == 24);
    assert!(size_of::<DxgiModeDesc1>() == 32);
    assert!(offset_of!(DxgiModeDesc1, width) == 0);
    assert!(offset_of!(DxgiModeDesc1, height) == 4);
    assert!(offset_of!(DxgiModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc1, format) == 16);
    assert!(offset_of!(DxgiModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc1, scaling) == 24);
    assert!(offset_of!(DxgiModeDesc1, stereo) == 28);
    assert!(size_of::<DxgiSurfaceDesc>() == 20);
    assert!(offset_of!(DxgiSurfaceDesc, width) == 0);
    assert!(offset_of!(DxgiSurfaceDesc, height) == 4);
    assert!(offset_of!(DxgiSurfaceDesc, format) == 8);
    assert!(offset_of!(DxgiSurfaceDesc, sample_desc) == 12);
    assert!(size_of::<DxgiMappedRect>() == 16);
    assert!(offset_of!(DxgiMappedRect, pitch) == 0);
    assert!(offset_of!(DxgiMappedRect, p_bits) == 8);
    assert!(size_of::<DxgiSharedResource>() == 8);
    assert!(offset_of!(DxgiSharedResource, handle) == 0);
    assert!(size_of::<DxgiFrameStatistics>() == 32);
    assert!(offset_of!(DxgiFrameStatistics, present_count) == 0);
    assert!(offset_of!(DxgiFrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(DxgiFrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(DxgiFrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(DxgiFrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<DxgiSwapChainDesc>() == 72);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(DxgiSwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(DxgiSwapChainDesc, output_window) == 48);
    assert!(offset_of!(DxgiSwapChainDesc, windowed) == 56);
    assert!(offset_of!(DxgiSwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(DxgiSwapChainDesc, flags) == 64);
    assert!(size_of::<DxgiSwapChainDesc1>() == 48);
    assert!(offset_of!(DxgiSwapChainDesc1, width) == 0);
    assert!(offset_of!(DxgiSwapChainDesc1, height) == 4);
    assert!(offset_of!(DxgiSwapChainDesc1, format) == 8);
    assert!(offset_of!(DxgiSwapChainDesc1, stereo) == 12);
    assert!(offset_of!(DxgiSwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(DxgiSwapChainDesc1, scaling) == 32);
    assert!(offset_of!(DxgiSwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(DxgiSwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(DxgiSwapChainDesc1, flags) == 44);
    assert!(size_of::<DxgiSwapChainFullscreenDesc>() == 20);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, windowed) == 16);
    assert!(size_of::<DxgiPresentParameters>() == 32);
    assert!(offset_of!(DxgiPresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(DxgiPresentParameters, p_dirty_rects) == 8);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_rect) == 16);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_offset) == 24);
    assert!(size_of::<DxgiMatrix3x2F>() == 24);
    assert!(offset_of!(DxgiMatrix3x2F, m_11) == 0);
    assert!(offset_of!(DxgiMatrix3x2F, m_12) == 4);
    assert!(offset_of!(DxgiMatrix3x2F, m_21) == 8);
    assert!(offset_of!(DxgiMatrix3x2F, m_22) == 12);
    assert!(offset_of!(DxgiMatrix3x2F, m_31) == 16);
    assert!(offset_of!(DxgiMatrix3x2F, m_32) == 20);
    assert!(size_of::<DxgiHdrMetadataHdr10>() == 28);
    assert!(offset_of!(DxgiHdrMetadataHdr10, red_primary) == 0);
    assert!(offset_of!(DxgiHdrMetadataHdr10, green_primary) == 4);
    assert!(offset_of!(DxgiHdrMetadataHdr10, blue_primary) == 8);
    assert!(offset_of!(DxgiHdrMetadataHdr10, white_point) == 12);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_mastering_luminance) == 16);
    assert!(offset_of!(DxgiHdrMetadataHdr10, min_mastering_luminance) == 20);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_content_light_level) == 24);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_frame_average_light_level) == 26);
    assert!(size_of::<D3d11Box>() == 24);
    assert!(offset_of!(D3d11Box, left) == 0);
    assert!(offset_of!(D3d11Box, top) == 4);
    assert!(offset_of!(D3d11Box, front) == 8);
    assert!(offset_of!(D3d11Box, right) == 12);
    assert!(offset_of!(D3d11Box, bottom) == 16);
    assert!(offset_of!(D3d11Box, back) == 20);
    assert!(size_of::<D3d11Viewport>() == 24);
    assert!(offset_of!(D3d11Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d11Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d11Viewport, width) == 8);
    assert!(offset_of!(D3d11Viewport, height) == 12);
    assert!(offset_of!(D3d11Viewport, min_depth) == 16);
    assert!(offset_of!(D3d11Viewport, max_depth) == 20);
    assert!(size_of::<D3d11MappedSubresource>() == 16);
    assert!(offset_of!(D3d11MappedSubresource, p_data) == 0);
    assert!(offset_of!(D3d11MappedSubresource, row_pitch) == 8);
    assert!(offset_of!(D3d11MappedSubresource, depth_pitch) == 12);
    assert!(size_of::<D3d11SubresourceData>() == 16);
    assert!(offset_of!(D3d11SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d11BufferDesc>() == 24);
    assert!(offset_of!(D3d11BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d11BufferDesc, usage) == 4);
    assert!(offset_of!(D3d11BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d11BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d11BufferDesc, misc_flags) == 16);
    assert!(offset_of!(D3d11BufferDesc, structure_byte_stride) == 20);
    assert!(size_of::<D3d11Texture1dDesc>() == 32);
    assert!(offset_of!(D3d11Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d11Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d11Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d11Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d11Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d11Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d11Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d11Texture2dDesc>() == 44);
    assert!(offset_of!(D3d11Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d11Texture2dDesc1>() == 48);
    assert!(offset_of!(D3d11Texture2dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc1, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc1, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc1, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc1, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc1, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc1, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc1, misc_flags) == 40);
    assert!(offset_of!(D3d11Texture2dDesc1, texture_layout) == 44);
    assert!(size_of::<D3d11Texture3dDesc>() == 36);
    assert!(offset_of!(D3d11Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d11Texture3dDesc1>() == 40);
    assert!(offset_of!(D3d11Texture3dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc1, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc1, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc1, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc1, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc1, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc1, misc_flags) == 32);
    assert!(offset_of!(D3d11Texture3dDesc1, texture_layout) == 36);
    assert!(size_of::<D3d11BufferSrv>() == 8);
    assert!(offset_of!(D3d11BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d11BufferexSrv>() == 12);
    assert!(offset_of!(D3d11BufferexSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferexSrv, num_elements) == 4);
    assert!(offset_of!(D3d11BufferexSrv, flags) == 8);
    assert!(size_of::<D3d11Tex1dSrv>() == 8);
    assert!(offset_of!(D3d11Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv>() == 8);
    assert!(offset_of!(D3d11Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv1>() == 12);
    assert!(offset_of!(D3d11Tex2dSrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dSrv1, plane_slice) == 8);
    assert!(size_of::<D3d11Tex2dArraySrv1>() == 20);
    assert!(offset_of!(D3d11Tex2dArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv1, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv1, array_size) == 12);
    assert!(offset_of!(D3d11Tex2dArraySrv1, plane_slice) == 16);
    assert!(size_of::<D3d11Tex3dSrv>() == 8);
    assert!(offset_of!(D3d11Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeSrv>() == 8);
    assert!(offset_of!(D3d11TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeArraySrv>() == 16);
    assert!(offset_of!(D3d11TexcubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11TexcubeArraySrv, first2d_array_face) == 8);
    assert!(offset_of!(D3d11TexcubeArraySrv, num_cubes) == 12);
    assert!(size_of::<D3d11Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d11ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d11ShaderResourceViewDesc1>() == 28);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d11BufferRtv>() == 8);
    assert!(offset_of!(D3d11BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d11BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d11Tex1dRtv>() == 4);
    assert!(offset_of!(D3d11Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv1>() == 8);
    assert!(offset_of!(D3d11Tex2dRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dRtv1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayRtv1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d11Tex3dRtv>() == 12);
    assert!(offset_of!(D3d11Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d11RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc1>() == 24);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, u) == 8);
    assert!(size_of::<D3d11Tex1dDsv>() == 4);
    assert!(offset_of!(D3d11Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d11DepthStencilViewDesc>() == 24);
    assert!(offset_of!(D3d11DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d11DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(D3d11DepthStencilViewDesc, u) == 12);
    assert!(size_of::<D3d11BufferUav>() == 12);
    assert!(offset_of!(D3d11BufferUav, first_element) == 0);
    assert!(offset_of!(D3d11BufferUav, num_elements) == 4);
    assert!(offset_of!(D3d11BufferUav, flags) == 8);
    assert!(size_of::<D3d11Tex1dUav>() == 4);
    assert!(offset_of!(D3d11Tex1dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav>() == 4);
    assert!(offset_of!(D3d11Tex2dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav1>() == 8);
    assert!(offset_of!(D3d11Tex2dUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dUav1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayUav1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayUav1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex3dUav>() == 12);
    assert!(offset_of!(D3d11Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dUav, w_size) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc>() == 20);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, u) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc1>() == 24);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, u) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc>() == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend_alpha) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, render_target_write_mask) == 28);
    assert!(size_of::<D3d11BlendDesc>() == 264);
    assert!(offset_of!(D3d11BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc, render_target) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc1>() == 40);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op_enable) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op_alpha) == 28);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op) == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, render_target_write_mask) == 36);
    assert!(size_of::<D3d11BlendDesc1>() == 328);
    assert!(offset_of!(D3d11BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d11DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d11DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d11DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d11DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d11RasterizerDesc>() == 40);
    assert!(offset_of!(D3d11RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d11RasterizerDesc1>() == 44);
    assert!(offset_of!(D3d11RasterizerDesc1, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc1, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc1, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc1, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc1, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc1, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc1, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc1, forced_sample_count) == 40);
    assert!(size_of::<D3d11RasterizerDesc2>() == 48);
    assert!(offset_of!(D3d11RasterizerDesc2, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc2, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc2, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc2, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc2, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc2, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc2, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc2, forced_sample_count) == 40);
    assert!(offset_of!(D3d11RasterizerDesc2, conservative_raster) == 44);
    assert!(size_of::<D3d11SamplerDesc>() == 52);
    assert!(offset_of!(D3d11SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d11SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d11SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d11SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d11SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d11SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d11SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d11SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d11SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d11SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d11InputElementDesc>() == 32);
    assert!(offset_of!(D3d11InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d11InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d11InputElementDesc, format) == 12);
    assert!(offset_of!(D3d11InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d11InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d11InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d11InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d11SoDeclarationEntry>() == 24);
    assert!(offset_of!(D3d11SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_name) == 8);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_index) == 16);
    assert!(offset_of!(D3d11SoDeclarationEntry, start_component) == 20);
    assert!(offset_of!(D3d11SoDeclarationEntry, component_count) == 21);
    assert!(offset_of!(D3d11SoDeclarationEntry, output_slot) == 22);
    assert!(size_of::<D3d11ClassInstanceDesc>() == 32);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_id) == 0);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_index) == 4);
    assert!(offset_of!(D3d11ClassInstanceDesc, type_id) == 8);
    assert!(offset_of!(D3d11ClassInstanceDesc, constant_buffer) == 12);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_constant_buffer_offset) == 16);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_texture) == 20);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_sampler) == 24);
    assert!(offset_of!(D3d11ClassInstanceDesc, created) == 28);
    assert!(size_of::<D3d11QueryDesc>() == 8);
    assert!(offset_of!(D3d11QueryDesc, query) == 0);
    assert!(offset_of!(D3d11QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d11QueryDesc1>() == 12);
    assert!(offset_of!(D3d11QueryDesc1, query) == 0);
    assert!(offset_of!(D3d11QueryDesc1, misc_flags) == 4);
    assert!(offset_of!(D3d11QueryDesc1, context_type) == 8);
    assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, hs_invocations) == 64);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ds_invocations) == 72);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, cs_invocations) == 80);
    assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d11QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d11QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d11CounterDesc>() == 8);
    assert!(offset_of!(D3d11CounterDesc, counter) == 0);
    assert!(offset_of!(D3d11CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d11CounterInfo>() == 12);
    assert!(offset_of!(D3d11CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d11CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d11CounterInfo, num_detectable_parallel_units) == 8);
    assert!(size_of::<D3d11TiledResourceCoordinate>() == 16);
    assert!(offset_of!(D3d11TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(D3d11TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(D3d11TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(D3d11TiledResourceCoordinate, subresource) == 12);
    assert!(size_of::<D3d11TileRegionSize>() == 16);
    assert!(offset_of!(D3d11TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(D3d11TileRegionSize, b_use_box) == 4);
    assert!(offset_of!(D3d11TileRegionSize, width) == 8);
    assert!(offset_of!(D3d11TileRegionSize, height) == 12);
    assert!(offset_of!(D3d11TileRegionSize, depth) == 14);
    assert!(size_of::<D3d11TileShape>() == 12);
    assert!(offset_of!(D3d11TileShape, width_in_texels) == 0);
    assert!(offset_of!(D3d11TileShape, height_in_texels) == 4);
    assert!(offset_of!(D3d11TileShape, depth_in_texels) == 8);
    assert!(size_of::<D3d11SubresourceTiling>() == 12);
    assert!(offset_of!(D3d11SubresourceTiling, width_in_tiles) == 0);
    assert!(offset_of!(D3d11SubresourceTiling, height_in_tiles) == 4);
    assert!(offset_of!(D3d11SubresourceTiling, depth_in_tiles) == 6);
    assert!(offset_of!(D3d11SubresourceTiling, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11PackedMipDesc>() == 12);
    assert!(offset_of!(D3d11PackedMipDesc, num_standard_mips) == 0);
    assert!(offset_of!(D3d11PackedMipDesc, num_packed_mips) == 1);
    assert!(offset_of!(D3d11PackedMipDesc, num_tiles_for_packed_mips) == 4);
    assert!(offset_of!(D3d11PackedMipDesc, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11DrawInstancedIndirectArgs>() == 16);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, vertex_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_vertex_location) == 8);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_instance_location) == 12);
    assert!(size_of::<D3d11DrawIndexedInstancedIndirectArgs>() == 20);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, index_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_index_location) == 8);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, base_vertex_location) == 12);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_instance_location) == 16);
    assert!(size_of::<D3d11FeatureDataThreading>() == 8);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_concurrent_creates) == 0);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_command_lists) == 4);
    assert!(size_of::<D3d11FeatureDataDoubles>() == 4);
    assert!(offset_of!(D3d11FeatureDataDoubles, double_precision_float_shader_ops) == 0);
    assert!(size_of::<D3d11FeatureDataFormatSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, out_format_support) == 4);
    assert!(size_of::<D3d11FeatureDataFormatSupport2>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, out_format_support2) == 4);
    assert!(size_of::<D3d11FeatureDataD3d10XHardwareOptions>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d10XHardwareOptions, compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options>() == 56);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, output_merger_logic_op) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, uav_only_rendering_forced_sample_count) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, discard_ap_is_seen_by_driver) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, flags_for_update_and_copy_seen_by_driver) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, clear_view) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, copy_with_overlap) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_partial_update) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_offsetting) == 28);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_constant_buffer) == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_buffer_srv) == 36);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, multisample_rtv_with_forced_sample_count_one) == 40);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, sad4_shader_instructions) == 44);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_doubles_shader_instructions) == 48);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_resource_sharing) == 52);
    assert!(size_of::<D3d11FeatureDataArchitectureInfo>() == 4);
    assert!(offset_of!(D3d11FeatureDataArchitectureInfo, tile_based_deferred_renderer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options, full_non_pow2_texture_support) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9ShadowSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9ShadowSupport, supports_depth_as_texture_with_less_equal_comparison_filter) == 0);
    assert!(size_of::<D3d11FeatureDataShaderMinPrecisionSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, pixel_shader_min_precision) == 0);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, all_other_shader_stages_min_precision) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, tiled_resources_tier) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, min_max_filtering) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, clear_view_also_supports_depth_only_formats) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, map_on_default_buffers) == 12);
    assert!(size_of::<D3d11FeatureDataD3d9SimpleInstancingSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9SimpleInstancingSupport, simple_instancing_supported) == 0);
    assert!(size_of::<D3d11FeatureDataMarkerSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataMarkerSupport, profile) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, full_non_pow2_texture_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, depth_as_texture_with_less_equal_comparison_filter_supported) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, simple_instancing_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, texture_cube_face_render_target_with_non_cube_depth_stencil_supported) == 12);
    assert!(size_of::<D3d11FeatureDataD3d11Options2>() == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ps_specified_stencil_ref_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, typed_uav_load_additional_formats) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ro_vs_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, conservative_rasterization_tier) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, tiled_resources_tier) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, map_on_default_textures) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, standard_swizzle) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, unified_memory_architecture) == 28);
    assert!(size_of::<D3d11FeatureDataD3d11Options3>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options3, vp_and_rt_array_index_from_any_shader_feeding_rasterizer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options4>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options4, extended_nv12_shared_texture_supported) == 0);
    assert!(size_of::<D3d11FeatureDataGpuVirtualAddressSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_resource) == 0);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_process) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options5>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options5, shared_resource_tier) == 0);
    assert!(size_of::<D3d11FeatureDataShaderCache>() == 4);
    assert!(offset_of!(D3d11FeatureDataShaderCache, support_flags) == 0);
    assert!(size_of::<D3d10Box>() == 24);
    assert!(offset_of!(D3d10Box, left) == 0);
    assert!(offset_of!(D3d10Box, top) == 4);
    assert!(offset_of!(D3d10Box, front) == 8);
    assert!(offset_of!(D3d10Box, right) == 12);
    assert!(offset_of!(D3d10Box, bottom) == 16);
    assert!(offset_of!(D3d10Box, back) == 20);
    assert!(size_of::<D3d10Viewport>() == 24);
    assert!(offset_of!(D3d10Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d10Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d10Viewport, width) == 8);
    assert!(offset_of!(D3d10Viewport, height) == 12);
    assert!(offset_of!(D3d10Viewport, min_depth) == 16);
    assert!(offset_of!(D3d10Viewport, max_depth) == 20);
    assert!(size_of::<D3d10MappedTexture2d>() == 16);
    assert!(offset_of!(D3d10MappedTexture2d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture2d, row_pitch) == 8);
    assert!(size_of::<D3d10MappedTexture3d>() == 16);
    assert!(offset_of!(D3d10MappedTexture3d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture3d, row_pitch) == 8);
    assert!(offset_of!(D3d10MappedTexture3d, depth_pitch) == 12);
    assert!(size_of::<D3d10SubresourceData>() == 16);
    assert!(offset_of!(D3d10SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d10BufferDesc>() == 20);
    assert!(offset_of!(D3d10BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d10BufferDesc, usage) == 4);
    assert!(offset_of!(D3d10BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d10BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d10BufferDesc, misc_flags) == 16);
    assert!(size_of::<D3d10Texture1dDesc>() == 32);
    assert!(offset_of!(D3d10Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d10Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d10Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d10Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d10Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d10Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d10Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d10Texture2dDesc>() == 44);
    assert!(offset_of!(D3d10Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d10Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d10Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d10Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d10Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d10Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d10Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d10Texture3dDesc>() == 36);
    assert!(offset_of!(D3d10Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d10Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d10Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d10Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d10Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d10Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d10BufferSrv>() == 8);
    assert!(offset_of!(D3d10BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d10BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dSrv>() == 8);
    assert!(offset_of!(D3d10Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex2dSrv>() == 8);
    assert!(offset_of!(D3d10Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex3dSrv>() == 8);
    assert!(offset_of!(D3d10Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeSrv>() == 8);
    assert!(offset_of!(D3d10TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeArraySrv1>() == 16);
    assert!(offset_of!(D3d10TexcubeArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d10TexcubeArraySrv1, first2d_array_face) == 8);
    assert!(offset_of!(D3d10TexcubeArraySrv1, num_cubes) == 12);
    assert!(size_of::<D3d10Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d10ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d10ShaderResourceViewDesc1>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d10BufferRtv>() == 8);
    assert!(offset_of!(D3d10BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d10BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dRtv>() == 4);
    assert!(offset_of!(D3d10Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d10Tex3dRtv>() == 12);
    assert!(offset_of!(D3d10Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d10Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d10RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d10RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d10RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d10Tex1dDsv>() == 4);
    assert!(offset_of!(D3d10Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d10DepthStencilViewDesc>() == 20);
    assert!(offset_of!(D3d10DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d10DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10DepthStencilViewDesc, u) == 8);
    assert!(size_of::<D3d10BlendDesc>() == 68);
    assert!(offset_of!(D3d10BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc, blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc, src_blend) == 36);
    assert!(offset_of!(D3d10BlendDesc, dest_blend) == 40);
    assert!(offset_of!(D3d10BlendDesc, blend_op) == 44);
    assert!(offset_of!(D3d10BlendDesc, src_blend_alpha) == 48);
    assert!(offset_of!(D3d10BlendDesc, dest_blend_alpha) == 52);
    assert!(offset_of!(D3d10BlendDesc, blend_op_alpha) == 56);
    assert!(offset_of!(D3d10BlendDesc, render_target_write_mask) == 60);
    assert!(size_of::<D3d10RenderTargetBlendDesc1>() == 32);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend) == 4);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend) == 8);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op) == 12);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend_alpha) == 16);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op_alpha) == 24);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, render_target_write_mask) == 28);
    assert!(size_of::<D3d10BlendDesc1>() == 264);
    assert!(offset_of!(D3d10BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d10DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d10DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d10DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d10DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d10RasterizerDesc>() == 40);
    assert!(offset_of!(D3d10RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d10RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d10RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d10RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d10RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d10RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d10RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d10RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d10SamplerDesc>() == 52);
    assert!(offset_of!(D3d10SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d10SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d10SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d10SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d10SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d10SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d10SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d10SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d10SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d10SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d10InputElementDesc>() == 32);
    assert!(offset_of!(D3d10InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d10InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d10InputElementDesc, format) == 12);
    assert!(offset_of!(D3d10InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d10InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d10InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d10InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d10SoDeclarationEntry>() == 16);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_name) == 0);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(D3d10SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(D3d10SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(D3d10SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<D3d10QueryDesc>() == 8);
    assert!(offset_of!(D3d10QueryDesc, query) == 0);
    assert!(offset_of!(D3d10QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d10QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d10QueryDataPipelineStatistics>() == 64);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(size_of::<D3d10QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d10QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d10QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d10CounterDesc>() == 8);
    assert!(offset_of!(D3d10CounterDesc, counter) == 0);
    assert!(offset_of!(D3d10CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d10CounterInfo>() == 12);
    assert!(offset_of!(D3d10CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d10CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d10CounterInfo, num_detectable_parallel_units) == 8);
};

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: () = {
    assert!(size_of::<Luid>() == 8);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<Rect>() == 16);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Point>() == 8);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
    assert!(size_of::<SecurityAttributes>() == 24);
    assert!(offset_of!(SecurityAttributes, n_length) == 0);
    assert!(offset_of!(SecurityAttributes, lp_security_descriptor) == 8);
    assert!(offset_of!(SecurityAttributes, b_inherit_handle) == 16);
    assert!(size_of::<DxgiRational>() == 8);
    assert!(offset_of!(DxgiRational, numerator) == 0);
    assert!(offset_of!(DxgiRational, denominator) == 4);
    assert!(size_of::<DxgiSampleDesc>() == 8);
    assert!(offset_of!(DxgiSampleDesc, count) == 0);
    assert!(offset_of!(DxgiSampleDesc, quality) == 4);
    assert!(size_of::<DxgiRgb>() == 12);
    assert!(offset_of!(DxgiRgb, red) == 0);
    assert!(offset_of!(DxgiRgb, green) == 4);
    assert!(offset_of!(DxgiRgb, blue) == 8);
    assert!(size_of::<DxgiRgba>() == 16);
    assert!(offset_of!(DxgiRgba, r) == 0);
    assert!(offset_of!(DxgiRgba, g) == 4);
    assert!(offset_of!(DxgiRgba, b) == 8);
    assert!(offset_of!(DxgiRgba, a) == 12);
    assert!(size_of::<DxgiGammaControl>() == 12324);
    assert!(offset_of!(DxgiGammaControl, scale) == 0);
    assert!(offset_of!(DxgiGammaControl, offset) == 12);
    assert!(offset_of!(DxgiGammaControl, gamma_curve) == 24);
    assert!(size_of::<DxgiGammaControlCapabilities>() == 4116);
    assert!(offset_of!(DxgiGammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(DxgiGammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(DxgiGammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(DxgiGammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(DxgiGammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<DxgiModeDesc>() == 28);
    assert!(offset_of!(DxgiModeDesc, width) == 0);
    assert!(offset_of!(DxgiModeDesc, height) == 4);
    assert!(offset_of!(DxgiModeDesc, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc, format) == 16);
    assert!(offset_of!(DxgiModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc, scaling) == 24);
    assert!(size_of::<DxgiModeDesc1>() == 32);
    assert!(offset_of!(DxgiModeDesc1, width) == 0);
    assert!(offset_of!(DxgiModeDesc1, height) == 4);
    assert!(offset_of!(DxgiModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc1, format) == 16);
    assert!(offset_of!(DxgiModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc1, scaling) == 24);
    assert!(offset_of!(DxgiModeDesc1, stereo) == 28);
    assert!(size_of::<DxgiSurfaceDesc>() == 20);
    assert!(offset_of!(DxgiSurfaceDesc, width) == 0);
    assert!(offset_of!(DxgiSurfaceDesc, height) == 4);
    assert!(offset_of!(DxgiSurfaceDesc, format) == 8);
    assert!(offset_of!(DxgiSurfaceDesc, sample_desc) == 12);
    assert!(size_of::<DxgiMappedRect>() == 16);
    assert!(offset_of!(DxgiMappedRect, pitch) == 0);
    assert!(offset_of!(DxgiMappedRect, p_bits) == 8);
    assert!(size_of::<DxgiSharedResource>() == 8);
    assert!(offset_of!(DxgiSharedResource, handle) == 0);
    assert!(size_of::<DxgiFrameStatistics>() == 32);
    assert!(offset_of!(DxgiFrameStatistics, present_count) == 0);
    assert!(offset_of!(DxgiFrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(DxgiFrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(DxgiFrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(DxgiFrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<DxgiSwapChainDesc>() == 72);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(DxgiSwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(DxgiSwapChainDesc, output_window) == 48);
    assert!(offset_of!(DxgiSwapChainDesc, windowed) == 56);
    assert!(offset_of!(DxgiSwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(DxgiSwapChainDesc, flags) == 64);
    assert!(size_of::<DxgiSwapChainDesc1>() == 48);
    assert!(offset_of!(DxgiSwapChainDesc1, width) == 0);
    assert!(offset_of!(DxgiSwapChainDesc1, height) == 4);
    assert!(offset_of!(DxgiSwapChainDesc1, format) == 8);
    assert!(offset_of!(DxgiSwapChainDesc1, stereo) == 12);
    assert!(offset_of!(DxgiSwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(DxgiSwapChainDesc1, scaling) == 32);
    assert!(offset_of!(DxgiSwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(DxgiSwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(DxgiSwapChainDesc1, flags) == 44);
    assert!(size_of::<DxgiSwapChainFullscreenDesc>() == 20);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, windowed) == 16);
    assert!(size_of::<DxgiPresentParameters>() == 32);
    assert!(offset_of!(DxgiPresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(DxgiPresentParameters, p_dirty_rects) == 8);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_rect) == 16);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_offset) == 24);
    assert!(size_of::<DxgiMatrix3x2F>() == 24);
    assert!(offset_of!(DxgiMatrix3x2F, m_11) == 0);
    assert!(offset_of!(DxgiMatrix3x2F, m_12) == 4);
    assert!(offset_of!(DxgiMatrix3x2F, m_21) == 8);
    assert!(offset_of!(DxgiMatrix3x2F, m_22) == 12);
    assert!(offset_of!(DxgiMatrix3x2F, m_31) == 16);
    assert!(offset_of!(DxgiMatrix3x2F, m_32) == 20);
    assert!(size_of::<DxgiHdrMetadataHdr10>() == 28);
    assert!(offset_of!(DxgiHdrMetadataHdr10, red_primary) == 0);
    assert!(offset_of!(DxgiHdrMetadataHdr10, green_primary) == 4);
    assert!(offset_of!(DxgiHdrMetadataHdr10, blue_primary) == 8);
    assert!(offset_of!(DxgiHdrMetadataHdr10, white_point) == 12);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_mastering_luminance) == 16);
    assert!(offset_of!(DxgiHdrMetadataHdr10, min_mastering_luminance) == 20);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_content_light_level) == 24);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_frame_average_light_level) == 26);
    assert!(size_of::<D3d11Box>() == 24);
    assert!(offset_of!(D3d11Box, left) == 0);
    assert!(offset_of!(D3d11Box, top) == 4);
    assert!(offset_of!(D3d11Box, front) == 8);
    assert!(offset_of!(D3d11Box, right) == 12);
    assert!(offset_of!(D3d11Box, bottom) == 16);
    assert!(offset_of!(D3d11Box, back) == 20);
    assert!(size_of::<D3d11Viewport>() == 24);
    assert!(offset_of!(D3d11Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d11Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d11Viewport, width) == 8);
    assert!(offset_of!(D3d11Viewport, height) == 12);
    assert!(offset_of!(D3d11Viewport, min_depth) == 16);
    assert!(offset_of!(D3d11Viewport, max_depth) == 20);
    assert!(size_of::<D3d11MappedSubresource>() == 16);
    assert!(offset_of!(D3d11MappedSubresource, p_data) == 0);
    assert!(offset_of!(D3d11MappedSubresource, row_pitch) == 8);
    assert!(offset_of!(D3d11MappedSubresource, depth_pitch) == 12);
    assert!(size_of::<D3d11SubresourceData>() == 16);
    assert!(offset_of!(D3d11SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d11BufferDesc>() == 24);
    assert!(offset_of!(D3d11BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d11BufferDesc, usage) == 4);
    assert!(offset_of!(D3d11BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d11BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d11BufferDesc, misc_flags) == 16);
    assert!(offset_of!(D3d11BufferDesc, structure_byte_stride) == 20);
    assert!(size_of::<D3d11Texture1dDesc>() == 32);
    assert!(offset_of!(D3d11Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d11Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d11Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d11Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d11Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d11Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d11Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d11Texture2dDesc>() == 44);
    assert!(offset_of!(D3d11Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d11Texture2dDesc1>() == 48);
    assert!(offset_of!(D3d11Texture2dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc1, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc1, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc1, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc1, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc1, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc1, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc1, misc_flags) == 40);
    assert!(offset_of!(D3d11Texture2dDesc1, texture_layout) == 44);
    assert!(size_of::<D3d11Texture3dDesc>() == 36);
    assert!(offset_of!(D3d11Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d11Texture3dDesc1>() == 40);
    assert!(offset_of!(D3d11Texture3dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc1, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc1, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc1, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc1, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc1, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc1, misc_flags) == 32);
    assert!(offset_of!(D3d11Texture3dDesc1, texture_layout) == 36);
    assert!(size_of::<D3d11BufferSrv>() == 8);
    assert!(offset_of!(D3d11BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d11BufferexSrv>() == 12);
    assert!(offset_of!(D3d11BufferexSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferexSrv, num_elements) == 4);
    assert!(offset_of!(D3d11BufferexSrv, flags) == 8);
    assert!(size_of::<D3d11Tex1dSrv>() == 8);
    assert!(offset_of!(D3d11Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv>() == 8);
    assert!(offset_of!(D3d11Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv1>() == 12);
    assert!(offset_of!(D3d11Tex2dSrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dSrv1, plane_slice) == 8);
    assert!(size_of::<D3d11Tex2dArraySrv1>() == 20);
    assert!(offset_of!(D3d11Tex2dArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv1, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv1, array_size) == 12);
    assert!(offset_of!(D3d11Tex2dArraySrv1, plane_slice) == 16);
    assert!(size_of::<D3d11Tex3dSrv>() == 8);
    assert!(offset_of!(D3d11Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeSrv>() == 8);
    assert!(offset_of!(D3d11TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeArraySrv>() == 16);
    assert!(offset_of!(D3d11TexcubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11TexcubeArraySrv, first2d_array_face) == 8);
    assert!(offset_of!(D3d11TexcubeArraySrv, num_cubes) == 12);
    assert!(size_of::<D3d11Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d11ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d11ShaderResourceViewDesc1>() == 28);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d11BufferRtv>() == 8);
    assert!(offset_of!(D3d11BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d11BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d11Tex1dRtv>() == 4);
    assert!(offset_of!(D3d11Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv1>() == 8);
    assert!(offset_of!(D3d11Tex2dRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dRtv1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayRtv1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d11Tex3dRtv>() == 12);
    assert!(offset_of!(D3d11Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d11RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc1>() == 24);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, u) == 8);
    assert!(size_of::<D3d11Tex1dDsv>() == 4);
    assert!(offset_of!(D3d11Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d11DepthStencilViewDesc>() == 24);
    assert!(offset_of!(D3d11DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d11DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(D3d11DepthStencilViewDesc, u) == 12);
    assert!(size_of::<D3d11BufferUav>() == 12);
    assert!(offset_of!(D3d11BufferUav, first_element) == 0);
    assert!(offset_of!(D3d11BufferUav, num_elements) == 4);
    assert!(offset_of!(D3d11BufferUav, flags) == 8);
    assert!(size_of::<D3d11Tex1dUav>() == 4);
    assert!(offset_of!(D3d11Tex1dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav>() == 4);
    assert!(offset_of!(D3d11Tex2dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav1>() == 8);
    assert!(offset_of!(D3d11Tex2dUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dUav1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayUav1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayUav1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex3dUav>() == 12);
    assert!(offset_of!(D3d11Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dUav, w_size) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc>() == 20);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, u) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc1>() == 24);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, u) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc>() == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend_alpha) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, render_target_write_mask) == 28);
    assert!(size_of::<D3d11BlendDesc>() == 264);
    assert!(offset_of!(D3d11BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc, render_target) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc1>() == 40);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op_enable) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op_alpha) == 28);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op) == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, render_target_write_mask) == 36);
    assert!(size_of::<D3d11BlendDesc1>() == 328);
    assert!(offset_of!(D3d11BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d11DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d11DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d11DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d11DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d11RasterizerDesc>() == 40);
    assert!(offset_of!(D3d11RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d11RasterizerDesc1>() == 44);
    assert!(offset_of!(D3d11RasterizerDesc1, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc1, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc1, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc1, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc1, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc1, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc1, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc1, forced_sample_count) == 40);
    assert!(size_of::<D3d11RasterizerDesc2>() == 48);
    assert!(offset_of!(D3d11RasterizerDesc2, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc2, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc2, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc2, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc2, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc2, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc2, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc2, forced_sample_count) == 40);
    assert!(offset_of!(D3d11RasterizerDesc2, conservative_raster) == 44);
    assert!(size_of::<D3d11SamplerDesc>() == 52);
    assert!(offset_of!(D3d11SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d11SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d11SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d11SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d11SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d11SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d11SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d11SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d11SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d11SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d11InputElementDesc>() == 32);
    assert!(offset_of!(D3d11InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d11InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d11InputElementDesc, format) == 12);
    assert!(offset_of!(D3d11InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d11InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d11InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d11InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d11SoDeclarationEntry>() == 24);
    assert!(offset_of!(D3d11SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_name) == 8);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_index) == 16);
    assert!(offset_of!(D3d11SoDeclarationEntry, start_component) == 20);
    assert!(offset_of!(D3d11SoDeclarationEntry, component_count) == 21);
    assert!(offset_of!(D3d11SoDeclarationEntry, output_slot) == 22);
    assert!(size_of::<D3d11ClassInstanceDesc>() == 32);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_id) == 0);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_index) == 4);
    assert!(offset_of!(D3d11ClassInstanceDesc, type_id) == 8);
    assert!(offset_of!(D3d11ClassInstanceDesc, constant_buffer) == 12);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_constant_buffer_offset) == 16);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_texture) == 20);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_sampler) == 24);
    assert!(offset_of!(D3d11ClassInstanceDesc, created) == 28);
    assert!(size_of::<D3d11QueryDesc>() == 8);
    assert!(offset_of!(D3d11QueryDesc, query) == 0);
    assert!(offset_of!(D3d11QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d11QueryDesc1>() == 12);
    assert!(offset_of!(D3d11QueryDesc1, query) == 0);
    assert!(offset_of!(D3d11QueryDesc1, misc_flags) == 4);
    assert!(offset_of!(D3d11QueryDesc1, context_type) == 8);
    assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, hs_invocations) == 64);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ds_invocations) == 72);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, cs_invocations) == 80);
    assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d11QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d11QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d11CounterDesc>() == 8);
    assert!(offset_of!(D3d11CounterDesc, counter) == 0);
    assert!(offset_of!(D3d11CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d11CounterInfo>() == 12);
    assert!(offset_of!(D3d11CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d11CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d11CounterInfo, num_detectable_parallel_units) == 8);
    assert!(size_of::<D3d11TiledResourceCoordinate>() == 16);
    assert!(offset_of!(D3d11TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(D3d11TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(D3d11TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(D3d11TiledResourceCoordinate, subresource) == 12);
    assert!(size_of::<D3d11TileRegionSize>() == 16);
    assert!(offset_of!(D3d11TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(D3d11TileRegionSize, b_use_box) == 4);
    assert!(offset_of!(D3d11TileRegionSize, width) == 8);
    assert!(offset_of!(D3d11TileRegionSize, height) == 12);
    assert!(offset_of!(D3d11TileRegionSize, depth) == 14);
    assert!(size_of::<D3d11TileShape>() == 12);
    assert!(offset_of!(D3d11TileShape, width_in_texels) == 0);
    assert!(offset_of!(D3d11TileShape, height_in_texels) == 4);
    assert!(offset_of!(D3d11TileShape, depth_in_texels) == 8);
    assert!(size_of::<D3d11SubresourceTiling>() == 12);
    assert!(offset_of!(D3d11SubresourceTiling, width_in_tiles) == 0);
    assert!(offset_of!(D3d11SubresourceTiling, height_in_tiles) == 4);
    assert!(offset_of!(D3d11SubresourceTiling, depth_in_tiles) == 6);
    assert!(offset_of!(D3d11SubresourceTiling, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11PackedMipDesc>() == 12);
    assert!(offset_of!(D3d11PackedMipDesc, num_standard_mips) == 0);
    assert!(offset_of!(D3d11PackedMipDesc, num_packed_mips) == 1);
    assert!(offset_of!(D3d11PackedMipDesc, num_tiles_for_packed_mips) == 4);
    assert!(offset_of!(D3d11PackedMipDesc, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11DrawInstancedIndirectArgs>() == 16);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, vertex_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_vertex_location) == 8);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_instance_location) == 12);
    assert!(size_of::<D3d11DrawIndexedInstancedIndirectArgs>() == 20);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, index_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_index_location) == 8);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, base_vertex_location) == 12);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_instance_location) == 16);
    assert!(size_of::<D3d11FeatureDataThreading>() == 8);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_concurrent_creates) == 0);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_command_lists) == 4);
    assert!(size_of::<D3d11FeatureDataDoubles>() == 4);
    assert!(offset_of!(D3d11FeatureDataDoubles, double_precision_float_shader_ops) == 0);
    assert!(size_of::<D3d11FeatureDataFormatSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, out_format_support) == 4);
    assert!(size_of::<D3d11FeatureDataFormatSupport2>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, out_format_support2) == 4);
    assert!(size_of::<D3d11FeatureDataD3d10XHardwareOptions>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d10XHardwareOptions, compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options>() == 56);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, output_merger_logic_op) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, uav_only_rendering_forced_sample_count) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, discard_ap_is_seen_by_driver) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, flags_for_update_and_copy_seen_by_driver) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, clear_view) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, copy_with_overlap) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_partial_update) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_offsetting) == 28);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_constant_buffer) == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_buffer_srv) == 36);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, multisample_rtv_with_forced_sample_count_one) == 40);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, sad4_shader_instructions) == 44);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_doubles_shader_instructions) == 48);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_resource_sharing) == 52);
    assert!(size_of::<D3d11FeatureDataArchitectureInfo>() == 4);
    assert!(offset_of!(D3d11FeatureDataArchitectureInfo, tile_based_deferred_renderer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options, full_non_pow2_texture_support) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9ShadowSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9ShadowSupport, supports_depth_as_texture_with_less_equal_comparison_filter) == 0);
    assert!(size_of::<D3d11FeatureDataShaderMinPrecisionSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, pixel_shader_min_precision) == 0);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, all_other_shader_stages_min_precision) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, tiled_resources_tier) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, min_max_filtering) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, clear_view_also_supports_depth_only_formats) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, map_on_default_buffers) == 12);
    assert!(size_of::<D3d11FeatureDataD3d9SimpleInstancingSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9SimpleInstancingSupport, simple_instancing_supported) == 0);
    assert!(size_of::<D3d11FeatureDataMarkerSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataMarkerSupport, profile) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, full_non_pow2_texture_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, depth_as_texture_with_less_equal_comparison_filter_supported) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, simple_instancing_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, texture_cube_face_render_target_with_non_cube_depth_stencil_supported) == 12);
    assert!(size_of::<D3d11FeatureDataD3d11Options2>() == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ps_specified_stencil_ref_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, typed_uav_load_additional_formats) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ro_vs_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, conservative_rasterization_tier) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, tiled_resources_tier) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, map_on_default_textures) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, standard_swizzle) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, unified_memory_architecture) == 28);
    assert!(size_of::<D3d11FeatureDataD3d11Options3>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options3, vp_and_rt_array_index_from_any_shader_feeding_rasterizer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options4>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options4, extended_nv12_shared_texture_supported) == 0);
    assert!(size_of::<D3d11FeatureDataGpuVirtualAddressSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_resource) == 0);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_process) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options5>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options5, shared_resource_tier) == 0);
    assert!(size_of::<D3d11FeatureDataShaderCache>() == 4);
    assert!(offset_of!(D3d11FeatureDataShaderCache, support_flags) == 0);
    assert!(size_of::<D3d10Box>() == 24);
    assert!(offset_of!(D3d10Box, left) == 0);
    assert!(offset_of!(D3d10Box, top) == 4);
    assert!(offset_of!(D3d10Box, front) == 8);
    assert!(offset_of!(D3d10Box, right) == 12);
    assert!(offset_of!(D3d10Box, bottom) == 16);
    assert!(offset_of!(D3d10Box, back) == 20);
    assert!(size_of::<D3d10Viewport>() == 24);
    assert!(offset_of!(D3d10Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d10Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d10Viewport, width) == 8);
    assert!(offset_of!(D3d10Viewport, height) == 12);
    assert!(offset_of!(D3d10Viewport, min_depth) == 16);
    assert!(offset_of!(D3d10Viewport, max_depth) == 20);
    assert!(size_of::<D3d10MappedTexture2d>() == 16);
    assert!(offset_of!(D3d10MappedTexture2d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture2d, row_pitch) == 8);
    assert!(size_of::<D3d10MappedTexture3d>() == 16);
    assert!(offset_of!(D3d10MappedTexture3d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture3d, row_pitch) == 8);
    assert!(offset_of!(D3d10MappedTexture3d, depth_pitch) == 12);
    assert!(size_of::<D3d10SubresourceData>() == 16);
    assert!(offset_of!(D3d10SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d10BufferDesc>() == 20);
    assert!(offset_of!(D3d10BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d10BufferDesc, usage) == 4);
    assert!(offset_of!(D3d10BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d10BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d10BufferDesc, misc_flags) == 16);
    assert!(size_of::<D3d10Texture1dDesc>() == 32);
    assert!(offset_of!(D3d10Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d10Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d10Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d10Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d10Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d10Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d10Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d10Texture2dDesc>() == 44);
    assert!(offset_of!(D3d10Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d10Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d10Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d10Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d10Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d10Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d10Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d10Texture3dDesc>() == 36);
    assert!(offset_of!(D3d10Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d10Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d10Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d10Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d10Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d10Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d10BufferSrv>() == 8);
    assert!(offset_of!(D3d10BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d10BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dSrv>() == 8);
    assert!(offset_of!(D3d10Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex2dSrv>() == 8);
    assert!(offset_of!(D3d10Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex3dSrv>() == 8);
    assert!(offset_of!(D3d10Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeSrv>() == 8);
    assert!(offset_of!(D3d10TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeArraySrv1>() == 16);
    assert!(offset_of!(D3d10TexcubeArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d10TexcubeArraySrv1, first2d_array_face) == 8);
    assert!(offset_of!(D3d10TexcubeArraySrv1, num_cubes) == 12);
    assert!(size_of::<D3d10Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d10ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d10ShaderResourceViewDesc1>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d10BufferRtv>() == 8);
    assert!(offset_of!(D3d10BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d10BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dRtv>() == 4);
    assert!(offset_of!(D3d10Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d10Tex3dRtv>() == 12);
    assert!(offset_of!(D3d10Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d10Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d10RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d10RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d10RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d10Tex1dDsv>() == 4);
    assert!(offset_of!(D3d10Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d10DepthStencilViewDesc>() == 20);
    assert!(offset_of!(D3d10DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d10DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10DepthStencilViewDesc, u) == 8);
    assert!(size_of::<D3d10BlendDesc>() == 68);
    assert!(offset_of!(D3d10BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc, blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc, src_blend) == 36);
    assert!(offset_of!(D3d10BlendDesc, dest_blend) == 40);
    assert!(offset_of!(D3d10BlendDesc, blend_op) == 44);
    assert!(offset_of!(D3d10BlendDesc, src_blend_alpha) == 48);
    assert!(offset_of!(D3d10BlendDesc, dest_blend_alpha) == 52);
    assert!(offset_of!(D3d10BlendDesc, blend_op_alpha) == 56);
    assert!(offset_of!(D3d10BlendDesc, render_target_write_mask) == 60);
    assert!(size_of::<D3d10RenderTargetBlendDesc1>() == 32);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend) == 4);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend) == 8);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op) == 12);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend_alpha) == 16);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op_alpha) == 24);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, render_target_write_mask) == 28);
    assert!(size_of::<D3d10BlendDesc1>() == 264);
    assert!(offset_of!(D3d10BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d10DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d10DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d10DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d10DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d10RasterizerDesc>() == 40);
    assert!(offset_of!(D3d10RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d10RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d10RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d10RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d10RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d10RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d10RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d10RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d10SamplerDesc>() == 52);
    assert!(offset_of!(D3d10SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d10SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d10SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d10SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d10SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d10SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d10SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d10SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d10SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d10SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d10InputElementDesc>() == 32);
    assert!(offset_of!(D3d10InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d10InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d10InputElementDesc, format) == 12);
    assert!(offset_of!(D3d10InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d10InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d10InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d10InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d10SoDeclarationEntry>() == 16);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_name) == 0);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(D3d10SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(D3d10SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(D3d10SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<D3d10QueryDesc>() == 8);
    assert!(offset_of!(D3d10QueryDesc, query) == 0);
    assert!(offset_of!(D3d10QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d10QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d10QueryDataPipelineStatistics>() == 64);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(size_of::<D3d10QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d10QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d10QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d10CounterDesc>() == 8);
    assert!(offset_of!(D3d10CounterDesc, counter) == 0);
    assert!(offset_of!(D3d10CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d10CounterInfo>() == 12);
    assert!(offset_of!(D3d10CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d10CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d10CounterInfo, num_detectable_parallel_units) == 8);
};

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<Luid>() == 8);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<Rect>() == 16);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Point>() == 8);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
    assert!(size_of::<SecurityAttributes>() == 24);
    assert!(offset_of!(SecurityAttributes, n_length) == 0);
    assert!(offset_of!(SecurityAttributes, lp_security_descriptor) == 8);
    assert!(offset_of!(SecurityAttributes, b_inherit_handle) == 16);
    assert!(size_of::<DxgiRational>() == 8);
    assert!(offset_of!(DxgiRational, numerator) == 0);
    assert!(offset_of!(DxgiRational, denominator) == 4);
    assert!(size_of::<DxgiSampleDesc>() == 8);
    assert!(offset_of!(DxgiSampleDesc, count) == 0);
    assert!(offset_of!(DxgiSampleDesc, quality) == 4);
    assert!(size_of::<DxgiRgb>() == 12);
    assert!(offset_of!(DxgiRgb, red) == 0);
    assert!(offset_of!(DxgiRgb, green) == 4);
    assert!(offset_of!(DxgiRgb, blue) == 8);
    assert!(size_of::<DxgiRgba>() == 16);
    assert!(offset_of!(DxgiRgba, r) == 0);
    assert!(offset_of!(DxgiRgba, g) == 4);
    assert!(offset_of!(DxgiRgba, b) == 8);
    assert!(offset_of!(DxgiRgba, a) == 12);
    assert!(size_of::<DxgiGammaControl>() == 12324);
    assert!(offset_of!(DxgiGammaControl, scale) == 0);
    assert!(offset_of!(DxgiGammaControl, offset) == 12);
    assert!(offset_of!(DxgiGammaControl, gamma_curve) == 24);
    assert!(size_of::<DxgiGammaControlCapabilities>() == 4116);
    assert!(offset_of!(DxgiGammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(DxgiGammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(DxgiGammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(DxgiGammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(DxgiGammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<DxgiModeDesc>() == 28);
    assert!(offset_of!(DxgiModeDesc, width) == 0);
    assert!(offset_of!(DxgiModeDesc, height) == 4);
    assert!(offset_of!(DxgiModeDesc, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc, format) == 16);
    assert!(offset_of!(DxgiModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc, scaling) == 24);
    assert!(size_of::<DxgiModeDesc1>() == 32);
    assert!(offset_of!(DxgiModeDesc1, width) == 0);
    assert!(offset_of!(DxgiModeDesc1, height) == 4);
    assert!(offset_of!(DxgiModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc1, format) == 16);
    assert!(offset_of!(DxgiModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc1, scaling) == 24);
    assert!(offset_of!(DxgiModeDesc1, stereo) == 28);
    assert!(size_of::<DxgiSurfaceDesc>() == 20);
    assert!(offset_of!(DxgiSurfaceDesc, width) == 0);
    assert!(offset_of!(DxgiSurfaceDesc, height) == 4);
    assert!(offset_of!(DxgiSurfaceDesc, format) == 8);
    assert!(offset_of!(DxgiSurfaceDesc, sample_desc) == 12);
    assert!(size_of::<DxgiMappedRect>() == 16);
    assert!(offset_of!(DxgiMappedRect, pitch) == 0);
    assert!(offset_of!(DxgiMappedRect, p_bits) == 8);
    assert!(size_of::<DxgiSharedResource>() == 8);
    assert!(offset_of!(DxgiSharedResource, handle) == 0);
    assert!(size_of::<DxgiFrameStatistics>() == 32);
    assert!(offset_of!(DxgiFrameStatistics, present_count) == 0);
    assert!(offset_of!(DxgiFrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(DxgiFrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(DxgiFrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(DxgiFrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<DxgiSwapChainDesc>() == 72);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(DxgiSwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(DxgiSwapChainDesc, output_window) == 48);
    assert!(offset_of!(DxgiSwapChainDesc, windowed) == 56);
    assert!(offset_of!(DxgiSwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(DxgiSwapChainDesc, flags) == 64);
    assert!(size_of::<DxgiSwapChainDesc1>() == 48);
    assert!(offset_of!(DxgiSwapChainDesc1, width) == 0);
    assert!(offset_of!(DxgiSwapChainDesc1, height) == 4);
    assert!(offset_of!(DxgiSwapChainDesc1, format) == 8);
    assert!(offset_of!(DxgiSwapChainDesc1, stereo) == 12);
    assert!(offset_of!(DxgiSwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(DxgiSwapChainDesc1, scaling) == 32);
    assert!(offset_of!(DxgiSwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(DxgiSwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(DxgiSwapChainDesc1, flags) == 44);
    assert!(size_of::<DxgiSwapChainFullscreenDesc>() == 20);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, windowed) == 16);
    assert!(size_of::<DxgiPresentParameters>() == 32);
    assert!(offset_of!(DxgiPresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(DxgiPresentParameters, p_dirty_rects) == 8);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_rect) == 16);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_offset) == 24);
    assert!(size_of::<DxgiMatrix3x2F>() == 24);
    assert!(offset_of!(DxgiMatrix3x2F, m_11) == 0);
    assert!(offset_of!(DxgiMatrix3x2F, m_12) == 4);
    assert!(offset_of!(DxgiMatrix3x2F, m_21) == 8);
    assert!(offset_of!(DxgiMatrix3x2F, m_22) == 12);
    assert!(offset_of!(DxgiMatrix3x2F, m_31) == 16);
    assert!(offset_of!(DxgiMatrix3x2F, m_32) == 20);
    assert!(size_of::<DxgiHdrMetadataHdr10>() == 28);
    assert!(offset_of!(DxgiHdrMetadataHdr10, red_primary) == 0);
    assert!(offset_of!(DxgiHdrMetadataHdr10, green_primary) == 4);
    assert!(offset_of!(DxgiHdrMetadataHdr10, blue_primary) == 8);
    assert!(offset_of!(DxgiHdrMetadataHdr10, white_point) == 12);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_mastering_luminance) == 16);
    assert!(offset_of!(DxgiHdrMetadataHdr10, min_mastering_luminance) == 20);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_content_light_level) == 24);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_frame_average_light_level) == 26);
    assert!(size_of::<D3d11Box>() == 24);
    assert!(offset_of!(D3d11Box, left) == 0);
    assert!(offset_of!(D3d11Box, top) == 4);
    assert!(offset_of!(D3d11Box, front) == 8);
    assert!(offset_of!(D3d11Box, right) == 12);
    assert!(offset_of!(D3d11Box, bottom) == 16);
    assert!(offset_of!(D3d11Box, back) == 20);
    assert!(size_of::<D3d11Viewport>() == 24);
    assert!(offset_of!(D3d11Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d11Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d11Viewport, width) == 8);
    assert!(offset_of!(D3d11Viewport, height) == 12);
    assert!(offset_of!(D3d11Viewport, min_depth) == 16);
    assert!(offset_of!(D3d11Viewport, max_depth) == 20);
    assert!(size_of::<D3d11MappedSubresource>() == 16);
    assert!(offset_of!(D3d11MappedSubresource, p_data) == 0);
    assert!(offset_of!(D3d11MappedSubresource, row_pitch) == 8);
    assert!(offset_of!(D3d11MappedSubresource, depth_pitch) == 12);
    assert!(size_of::<D3d11SubresourceData>() == 16);
    assert!(offset_of!(D3d11SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d11BufferDesc>() == 24);
    assert!(offset_of!(D3d11BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d11BufferDesc, usage) == 4);
    assert!(offset_of!(D3d11BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d11BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d11BufferDesc, misc_flags) == 16);
    assert!(offset_of!(D3d11BufferDesc, structure_byte_stride) == 20);
    assert!(size_of::<D3d11Texture1dDesc>() == 32);
    assert!(offset_of!(D3d11Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d11Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d11Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d11Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d11Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d11Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d11Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d11Texture2dDesc>() == 44);
    assert!(offset_of!(D3d11Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d11Texture2dDesc1>() == 48);
    assert!(offset_of!(D3d11Texture2dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc1, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc1, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc1, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc1, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc1, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc1, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc1, misc_flags) == 40);
    assert!(offset_of!(D3d11Texture2dDesc1, texture_layout) == 44);
    assert!(size_of::<D3d11Texture3dDesc>() == 36);
    assert!(offset_of!(D3d11Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d11Texture3dDesc1>() == 40);
    assert!(offset_of!(D3d11Texture3dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc1, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc1, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc1, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc1, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc1, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc1, misc_flags) == 32);
    assert!(offset_of!(D3d11Texture3dDesc1, texture_layout) == 36);
    assert!(size_of::<D3d11BufferSrv>() == 8);
    assert!(offset_of!(D3d11BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d11BufferexSrv>() == 12);
    assert!(offset_of!(D3d11BufferexSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferexSrv, num_elements) == 4);
    assert!(offset_of!(D3d11BufferexSrv, flags) == 8);
    assert!(size_of::<D3d11Tex1dSrv>() == 8);
    assert!(offset_of!(D3d11Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv>() == 8);
    assert!(offset_of!(D3d11Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv1>() == 12);
    assert!(offset_of!(D3d11Tex2dSrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dSrv1, plane_slice) == 8);
    assert!(size_of::<D3d11Tex2dArraySrv1>() == 20);
    assert!(offset_of!(D3d11Tex2dArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv1, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv1, array_size) == 12);
    assert!(offset_of!(D3d11Tex2dArraySrv1, plane_slice) == 16);
    assert!(size_of::<D3d11Tex3dSrv>() == 8);
    assert!(offset_of!(D3d11Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeSrv>() == 8);
    assert!(offset_of!(D3d11TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeArraySrv>() == 16);
    assert!(offset_of!(D3d11TexcubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11TexcubeArraySrv, first2d_array_face) == 8);
    assert!(offset_of!(D3d11TexcubeArraySrv, num_cubes) == 12);
    assert!(size_of::<D3d11Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d11ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d11ShaderResourceViewDesc1>() == 28);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d11BufferRtv>() == 8);
    assert!(offset_of!(D3d11BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d11BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d11Tex1dRtv>() == 4);
    assert!(offset_of!(D3d11Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv1>() == 8);
    assert!(offset_of!(D3d11Tex2dRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dRtv1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayRtv1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d11Tex3dRtv>() == 12);
    assert!(offset_of!(D3d11Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d11RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc1>() == 24);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, u) == 8);
    assert!(size_of::<D3d11Tex1dDsv>() == 4);
    assert!(offset_of!(D3d11Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d11DepthStencilViewDesc>() == 24);
    assert!(offset_of!(D3d11DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d11DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(D3d11DepthStencilViewDesc, u) == 12);
    assert!(size_of::<D3d11BufferUav>() == 12);
    assert!(offset_of!(D3d11BufferUav, first_element) == 0);
    assert!(offset_of!(D3d11BufferUav, num_elements) == 4);
    assert!(offset_of!(D3d11BufferUav, flags) == 8);
    assert!(size_of::<D3d11Tex1dUav>() == 4);
    assert!(offset_of!(D3d11Tex1dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav>() == 4);
    assert!(offset_of!(D3d11Tex2dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav1>() == 8);
    assert!(offset_of!(D3d11Tex2dUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dUav1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayUav1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayUav1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex3dUav>() == 12);
    assert!(offset_of!(D3d11Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dUav, w_size) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc>() == 20);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, u) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc1>() == 24);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, u) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc>() == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend_alpha) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, render_target_write_mask) == 28);
    assert!(size_of::<D3d11BlendDesc>() == 264);
    assert!(offset_of!(D3d11BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc, render_target) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc1>() == 40);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op_enable) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op_alpha) == 28);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op) == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, render_target_write_mask) == 36);
    assert!(size_of::<D3d11BlendDesc1>() == 328);
    assert!(offset_of!(D3d11BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d11DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d11DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d11DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d11DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d11RasterizerDesc>() == 40);
    assert!(offset_of!(D3d11RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d11RasterizerDesc1>() == 44);
    assert!(offset_of!(D3d11RasterizerDesc1, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc1, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc1, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc1, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc1, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc1, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc1, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc1, forced_sample_count) == 40);
    assert!(size_of::<D3d11RasterizerDesc2>() == 48);
    assert!(offset_of!(D3d11RasterizerDesc2, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc2, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc2, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc2, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc2, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc2, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc2, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc2, forced_sample_count) == 40);
    assert!(offset_of!(D3d11RasterizerDesc2, conservative_raster) == 44);
    assert!(size_of::<D3d11SamplerDesc>() == 52);
    assert!(offset_of!(D3d11SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d11SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d11SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d11SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d11SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d11SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d11SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d11SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d11SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d11SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d11InputElementDesc>() == 32);
    assert!(offset_of!(D3d11InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d11InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d11InputElementDesc, format) == 12);
    assert!(offset_of!(D3d11InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d11InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d11InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d11InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d11SoDeclarationEntry>() == 24);
    assert!(offset_of!(D3d11SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_name) == 8);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_index) == 16);
    assert!(offset_of!(D3d11SoDeclarationEntry, start_component) == 20);
    assert!(offset_of!(D3d11SoDeclarationEntry, component_count) == 21);
    assert!(offset_of!(D3d11SoDeclarationEntry, output_slot) == 22);
    assert!(size_of::<D3d11ClassInstanceDesc>() == 32);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_id) == 0);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_index) == 4);
    assert!(offset_of!(D3d11ClassInstanceDesc, type_id) == 8);
    assert!(offset_of!(D3d11ClassInstanceDesc, constant_buffer) == 12);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_constant_buffer_offset) == 16);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_texture) == 20);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_sampler) == 24);
    assert!(offset_of!(D3d11ClassInstanceDesc, created) == 28);
    assert!(size_of::<D3d11QueryDesc>() == 8);
    assert!(offset_of!(D3d11QueryDesc, query) == 0);
    assert!(offset_of!(D3d11QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d11QueryDesc1>() == 12);
    assert!(offset_of!(D3d11QueryDesc1, query) == 0);
    assert!(offset_of!(D3d11QueryDesc1, misc_flags) == 4);
    assert!(offset_of!(D3d11QueryDesc1, context_type) == 8);
    assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, hs_invocations) == 64);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ds_invocations) == 72);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, cs_invocations) == 80);
    assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d11QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d11QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d11CounterDesc>() == 8);
    assert!(offset_of!(D3d11CounterDesc, counter) == 0);
    assert!(offset_of!(D3d11CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d11CounterInfo>() == 12);
    assert!(offset_of!(D3d11CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d11CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d11CounterInfo, num_detectable_parallel_units) == 8);
    assert!(size_of::<D3d11TiledResourceCoordinate>() == 16);
    assert!(offset_of!(D3d11TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(D3d11TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(D3d11TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(D3d11TiledResourceCoordinate, subresource) == 12);
    assert!(size_of::<D3d11TileRegionSize>() == 16);
    assert!(offset_of!(D3d11TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(D3d11TileRegionSize, b_use_box) == 4);
    assert!(offset_of!(D3d11TileRegionSize, width) == 8);
    assert!(offset_of!(D3d11TileRegionSize, height) == 12);
    assert!(offset_of!(D3d11TileRegionSize, depth) == 14);
    assert!(size_of::<D3d11TileShape>() == 12);
    assert!(offset_of!(D3d11TileShape, width_in_texels) == 0);
    assert!(offset_of!(D3d11TileShape, height_in_texels) == 4);
    assert!(offset_of!(D3d11TileShape, depth_in_texels) == 8);
    assert!(size_of::<D3d11SubresourceTiling>() == 12);
    assert!(offset_of!(D3d11SubresourceTiling, width_in_tiles) == 0);
    assert!(offset_of!(D3d11SubresourceTiling, height_in_tiles) == 4);
    assert!(offset_of!(D3d11SubresourceTiling, depth_in_tiles) == 6);
    assert!(offset_of!(D3d11SubresourceTiling, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11PackedMipDesc>() == 12);
    assert!(offset_of!(D3d11PackedMipDesc, num_standard_mips) == 0);
    assert!(offset_of!(D3d11PackedMipDesc, num_packed_mips) == 1);
    assert!(offset_of!(D3d11PackedMipDesc, num_tiles_for_packed_mips) == 4);
    assert!(offset_of!(D3d11PackedMipDesc, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11DrawInstancedIndirectArgs>() == 16);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, vertex_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_vertex_location) == 8);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_instance_location) == 12);
    assert!(size_of::<D3d11DrawIndexedInstancedIndirectArgs>() == 20);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, index_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_index_location) == 8);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, base_vertex_location) == 12);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_instance_location) == 16);
    assert!(size_of::<D3d11FeatureDataThreading>() == 8);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_concurrent_creates) == 0);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_command_lists) == 4);
    assert!(size_of::<D3d11FeatureDataDoubles>() == 4);
    assert!(offset_of!(D3d11FeatureDataDoubles, double_precision_float_shader_ops) == 0);
    assert!(size_of::<D3d11FeatureDataFormatSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, out_format_support) == 4);
    assert!(size_of::<D3d11FeatureDataFormatSupport2>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, out_format_support2) == 4);
    assert!(size_of::<D3d11FeatureDataD3d10XHardwareOptions>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d10XHardwareOptions, compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options>() == 56);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, output_merger_logic_op) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, uav_only_rendering_forced_sample_count) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, discard_ap_is_seen_by_driver) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, flags_for_update_and_copy_seen_by_driver) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, clear_view) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, copy_with_overlap) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_partial_update) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_offsetting) == 28);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_constant_buffer) == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_buffer_srv) == 36);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, multisample_rtv_with_forced_sample_count_one) == 40);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, sad4_shader_instructions) == 44);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_doubles_shader_instructions) == 48);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_resource_sharing) == 52);
    assert!(size_of::<D3d11FeatureDataArchitectureInfo>() == 4);
    assert!(offset_of!(D3d11FeatureDataArchitectureInfo, tile_based_deferred_renderer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options, full_non_pow2_texture_support) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9ShadowSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9ShadowSupport, supports_depth_as_texture_with_less_equal_comparison_filter) == 0);
    assert!(size_of::<D3d11FeatureDataShaderMinPrecisionSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, pixel_shader_min_precision) == 0);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, all_other_shader_stages_min_precision) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, tiled_resources_tier) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, min_max_filtering) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, clear_view_also_supports_depth_only_formats) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, map_on_default_buffers) == 12);
    assert!(size_of::<D3d11FeatureDataD3d9SimpleInstancingSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9SimpleInstancingSupport, simple_instancing_supported) == 0);
    assert!(size_of::<D3d11FeatureDataMarkerSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataMarkerSupport, profile) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, full_non_pow2_texture_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, depth_as_texture_with_less_equal_comparison_filter_supported) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, simple_instancing_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, texture_cube_face_render_target_with_non_cube_depth_stencil_supported) == 12);
    assert!(size_of::<D3d11FeatureDataD3d11Options2>() == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ps_specified_stencil_ref_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, typed_uav_load_additional_formats) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ro_vs_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, conservative_rasterization_tier) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, tiled_resources_tier) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, map_on_default_textures) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, standard_swizzle) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, unified_memory_architecture) == 28);
    assert!(size_of::<D3d11FeatureDataD3d11Options3>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options3, vp_and_rt_array_index_from_any_shader_feeding_rasterizer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options4>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options4, extended_nv12_shared_texture_supported) == 0);
    assert!(size_of::<D3d11FeatureDataGpuVirtualAddressSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_resource) == 0);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_process) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options5>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options5, shared_resource_tier) == 0);
    assert!(size_of::<D3d11FeatureDataShaderCache>() == 4);
    assert!(offset_of!(D3d11FeatureDataShaderCache, support_flags) == 0);
    assert!(size_of::<D3d10Box>() == 24);
    assert!(offset_of!(D3d10Box, left) == 0);
    assert!(offset_of!(D3d10Box, top) == 4);
    assert!(offset_of!(D3d10Box, front) == 8);
    assert!(offset_of!(D3d10Box, right) == 12);
    assert!(offset_of!(D3d10Box, bottom) == 16);
    assert!(offset_of!(D3d10Box, back) == 20);
    assert!(size_of::<D3d10Viewport>() == 24);
    assert!(offset_of!(D3d10Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d10Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d10Viewport, width) == 8);
    assert!(offset_of!(D3d10Viewport, height) == 12);
    assert!(offset_of!(D3d10Viewport, min_depth) == 16);
    assert!(offset_of!(D3d10Viewport, max_depth) == 20);
    assert!(size_of::<D3d10MappedTexture2d>() == 16);
    assert!(offset_of!(D3d10MappedTexture2d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture2d, row_pitch) == 8);
    assert!(size_of::<D3d10MappedTexture3d>() == 16);
    assert!(offset_of!(D3d10MappedTexture3d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture3d, row_pitch) == 8);
    assert!(offset_of!(D3d10MappedTexture3d, depth_pitch) == 12);
    assert!(size_of::<D3d10SubresourceData>() == 16);
    assert!(offset_of!(D3d10SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d10BufferDesc>() == 20);
    assert!(offset_of!(D3d10BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d10BufferDesc, usage) == 4);
    assert!(offset_of!(D3d10BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d10BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d10BufferDesc, misc_flags) == 16);
    assert!(size_of::<D3d10Texture1dDesc>() == 32);
    assert!(offset_of!(D3d10Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d10Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d10Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d10Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d10Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d10Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d10Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d10Texture2dDesc>() == 44);
    assert!(offset_of!(D3d10Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d10Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d10Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d10Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d10Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d10Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d10Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d10Texture3dDesc>() == 36);
    assert!(offset_of!(D3d10Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d10Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d10Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d10Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d10Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d10Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d10BufferSrv>() == 8);
    assert!(offset_of!(D3d10BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d10BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dSrv>() == 8);
    assert!(offset_of!(D3d10Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex2dSrv>() == 8);
    assert!(offset_of!(D3d10Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex3dSrv>() == 8);
    assert!(offset_of!(D3d10Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeSrv>() == 8);
    assert!(offset_of!(D3d10TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeArraySrv1>() == 16);
    assert!(offset_of!(D3d10TexcubeArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d10TexcubeArraySrv1, first2d_array_face) == 8);
    assert!(offset_of!(D3d10TexcubeArraySrv1, num_cubes) == 12);
    assert!(size_of::<D3d10Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d10ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d10ShaderResourceViewDesc1>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d10BufferRtv>() == 8);
    assert!(offset_of!(D3d10BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d10BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dRtv>() == 4);
    assert!(offset_of!(D3d10Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d10Tex3dRtv>() == 12);
    assert!(offset_of!(D3d10Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d10Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d10RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d10RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d10RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d10Tex1dDsv>() == 4);
    assert!(offset_of!(D3d10Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d10DepthStencilViewDesc>() == 20);
    assert!(offset_of!(D3d10DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d10DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10DepthStencilViewDesc, u) == 8);
    assert!(size_of::<D3d10BlendDesc>() == 68);
    assert!(offset_of!(D3d10BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc, blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc, src_blend) == 36);
    assert!(offset_of!(D3d10BlendDesc, dest_blend) == 40);
    assert!(offset_of!(D3d10BlendDesc, blend_op) == 44);
    assert!(offset_of!(D3d10BlendDesc, src_blend_alpha) == 48);
    assert!(offset_of!(D3d10BlendDesc, dest_blend_alpha) == 52);
    assert!(offset_of!(D3d10BlendDesc, blend_op_alpha) == 56);
    assert!(offset_of!(D3d10BlendDesc, render_target_write_mask) == 60);
    assert!(size_of::<D3d10RenderTargetBlendDesc1>() == 32);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend) == 4);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend) == 8);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op) == 12);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend_alpha) == 16);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op_alpha) == 24);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, render_target_write_mask) == 28);
    assert!(size_of::<D3d10BlendDesc1>() == 264);
    assert!(offset_of!(D3d10BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d10DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d10DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d10DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d10DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d10RasterizerDesc>() == 40);
    assert!(offset_of!(D3d10RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d10RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d10RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d10RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d10RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d10RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d10RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d10RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d10SamplerDesc>() == 52);
    assert!(offset_of!(D3d10SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d10SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d10SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d10SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d10SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d10SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d10SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d10SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d10SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d10SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d10InputElementDesc>() == 32);
    assert!(offset_of!(D3d10InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d10InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d10InputElementDesc, format) == 12);
    assert!(offset_of!(D3d10InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d10InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d10InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d10InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d10SoDeclarationEntry>() == 16);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_name) == 0);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(D3d10SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(D3d10SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(D3d10SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<D3d10QueryDesc>() == 8);
    assert!(offset_of!(D3d10QueryDesc, query) == 0);
    assert!(offset_of!(D3d10QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d10QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d10QueryDataPipelineStatistics>() == 64);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(size_of::<D3d10QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d10QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d10QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d10CounterDesc>() == 8);
    assert!(offset_of!(D3d10CounterDesc, counter) == 0);
    assert!(offset_of!(D3d10CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d10CounterInfo>() == 12);
    assert!(offset_of!(D3d10CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d10CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d10CounterInfo, num_detectable_parallel_units) == 8);
};

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(size_of::<Luid>() == 8);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<Rect>() == 16);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Point>() == 8);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
    assert!(size_of::<SecurityAttributes>() == 12);
    assert!(offset_of!(SecurityAttributes, n_length) == 0);
    assert!(offset_of!(SecurityAttributes, lp_security_descriptor) == 4);
    assert!(offset_of!(SecurityAttributes, b_inherit_handle) == 8);
    assert!(size_of::<DxgiRational>() == 8);
    assert!(offset_of!(DxgiRational, numerator) == 0);
    assert!(offset_of!(DxgiRational, denominator) == 4);
    assert!(size_of::<DxgiSampleDesc>() == 8);
    assert!(offset_of!(DxgiSampleDesc, count) == 0);
    assert!(offset_of!(DxgiSampleDesc, quality) == 4);
    assert!(size_of::<DxgiRgb>() == 12);
    assert!(offset_of!(DxgiRgb, red) == 0);
    assert!(offset_of!(DxgiRgb, green) == 4);
    assert!(offset_of!(DxgiRgb, blue) == 8);
    assert!(size_of::<DxgiRgba>() == 16);
    assert!(offset_of!(DxgiRgba, r) == 0);
    assert!(offset_of!(DxgiRgba, g) == 4);
    assert!(offset_of!(DxgiRgba, b) == 8);
    assert!(offset_of!(DxgiRgba, a) == 12);
    assert!(size_of::<DxgiGammaControl>() == 12324);
    assert!(offset_of!(DxgiGammaControl, scale) == 0);
    assert!(offset_of!(DxgiGammaControl, offset) == 12);
    assert!(offset_of!(DxgiGammaControl, gamma_curve) == 24);
    assert!(size_of::<DxgiGammaControlCapabilities>() == 4116);
    assert!(offset_of!(DxgiGammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(DxgiGammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(DxgiGammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(DxgiGammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(DxgiGammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<DxgiModeDesc>() == 28);
    assert!(offset_of!(DxgiModeDesc, width) == 0);
    assert!(offset_of!(DxgiModeDesc, height) == 4);
    assert!(offset_of!(DxgiModeDesc, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc, format) == 16);
    assert!(offset_of!(DxgiModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc, scaling) == 24);
    assert!(size_of::<DxgiModeDesc1>() == 32);
    assert!(offset_of!(DxgiModeDesc1, width) == 0);
    assert!(offset_of!(DxgiModeDesc1, height) == 4);
    assert!(offset_of!(DxgiModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc1, format) == 16);
    assert!(offset_of!(DxgiModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc1, scaling) == 24);
    assert!(offset_of!(DxgiModeDesc1, stereo) == 28);
    assert!(size_of::<DxgiSurfaceDesc>() == 20);
    assert!(offset_of!(DxgiSurfaceDesc, width) == 0);
    assert!(offset_of!(DxgiSurfaceDesc, height) == 4);
    assert!(offset_of!(DxgiSurfaceDesc, format) == 8);
    assert!(offset_of!(DxgiSurfaceDesc, sample_desc) == 12);
    assert!(size_of::<DxgiMappedRect>() == 8);
    assert!(offset_of!(DxgiMappedRect, pitch) == 0);
    assert!(offset_of!(DxgiMappedRect, p_bits) == 4);
    assert!(size_of::<DxgiSharedResource>() == 4);
    assert!(offset_of!(DxgiSharedResource, handle) == 0);
    assert!(size_of::<DxgiFrameStatistics>() == 32);
    assert!(offset_of!(DxgiFrameStatistics, present_count) == 0);
    assert!(offset_of!(DxgiFrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(DxgiFrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(DxgiFrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(DxgiFrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<DxgiSwapChainDesc>() == 60);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(DxgiSwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(DxgiSwapChainDesc, output_window) == 44);
    assert!(offset_of!(DxgiSwapChainDesc, windowed) == 48);
    assert!(offset_of!(DxgiSwapChainDesc, swap_effect) == 52);
    assert!(offset_of!(DxgiSwapChainDesc, flags) == 56);
    assert!(size_of::<DxgiSwapChainDesc1>() == 48);
    assert!(offset_of!(DxgiSwapChainDesc1, width) == 0);
    assert!(offset_of!(DxgiSwapChainDesc1, height) == 4);
    assert!(offset_of!(DxgiSwapChainDesc1, format) == 8);
    assert!(offset_of!(DxgiSwapChainDesc1, stereo) == 12);
    assert!(offset_of!(DxgiSwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(DxgiSwapChainDesc1, scaling) == 32);
    assert!(offset_of!(DxgiSwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(DxgiSwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(DxgiSwapChainDesc1, flags) == 44);
    assert!(size_of::<DxgiSwapChainFullscreenDesc>() == 20);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, windowed) == 16);
    assert!(size_of::<DxgiPresentParameters>() == 16);
    assert!(offset_of!(DxgiPresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(DxgiPresentParameters, p_dirty_rects) == 4);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_rect) == 8);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_offset) == 12);
    assert!(size_of::<DxgiMatrix3x2F>() == 24);
    assert!(offset_of!(DxgiMatrix3x2F, m_11) == 0);
    assert!(offset_of!(DxgiMatrix3x2F, m_12) == 4);
    assert!(offset_of!(DxgiMatrix3x2F, m_21) == 8);
    assert!(offset_of!(DxgiMatrix3x2F, m_22) == 12);
    assert!(offset_of!(DxgiMatrix3x2F, m_31) == 16);
    assert!(offset_of!(DxgiMatrix3x2F, m_32) == 20);
    assert!(size_of::<DxgiHdrMetadataHdr10>() == 28);
    assert!(offset_of!(DxgiHdrMetadataHdr10, red_primary) == 0);
    assert!(offset_of!(DxgiHdrMetadataHdr10, green_primary) == 4);
    assert!(offset_of!(DxgiHdrMetadataHdr10, blue_primary) == 8);
    assert!(offset_of!(DxgiHdrMetadataHdr10, white_point) == 12);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_mastering_luminance) == 16);
    assert!(offset_of!(DxgiHdrMetadataHdr10, min_mastering_luminance) == 20);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_content_light_level) == 24);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_frame_average_light_level) == 26);
    assert!(size_of::<D3d11Box>() == 24);
    assert!(offset_of!(D3d11Box, left) == 0);
    assert!(offset_of!(D3d11Box, top) == 4);
    assert!(offset_of!(D3d11Box, front) == 8);
    assert!(offset_of!(D3d11Box, right) == 12);
    assert!(offset_of!(D3d11Box, bottom) == 16);
    assert!(offset_of!(D3d11Box, back) == 20);
    assert!(size_of::<D3d11Viewport>() == 24);
    assert!(offset_of!(D3d11Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d11Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d11Viewport, width) == 8);
    assert!(offset_of!(D3d11Viewport, height) == 12);
    assert!(offset_of!(D3d11Viewport, min_depth) == 16);
    assert!(offset_of!(D3d11Viewport, max_depth) == 20);
    assert!(size_of::<D3d11MappedSubresource>() == 12);
    assert!(offset_of!(D3d11MappedSubresource, p_data) == 0);
    assert!(offset_of!(D3d11MappedSubresource, row_pitch) == 4);
    assert!(offset_of!(D3d11MappedSubresource, depth_pitch) == 8);
    assert!(size_of::<D3d11SubresourceData>() == 12);
    assert!(offset_of!(D3d11SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_pitch) == 4);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_slice_pitch) == 8);
    assert!(size_of::<D3d11BufferDesc>() == 24);
    assert!(offset_of!(D3d11BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d11BufferDesc, usage) == 4);
    assert!(offset_of!(D3d11BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d11BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d11BufferDesc, misc_flags) == 16);
    assert!(offset_of!(D3d11BufferDesc, structure_byte_stride) == 20);
    assert!(size_of::<D3d11Texture1dDesc>() == 32);
    assert!(offset_of!(D3d11Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d11Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d11Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d11Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d11Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d11Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d11Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d11Texture2dDesc>() == 44);
    assert!(offset_of!(D3d11Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d11Texture2dDesc1>() == 48);
    assert!(offset_of!(D3d11Texture2dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc1, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc1, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc1, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc1, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc1, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc1, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc1, misc_flags) == 40);
    assert!(offset_of!(D3d11Texture2dDesc1, texture_layout) == 44);
    assert!(size_of::<D3d11Texture3dDesc>() == 36);
    assert!(offset_of!(D3d11Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d11Texture3dDesc1>() == 40);
    assert!(offset_of!(D3d11Texture3dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc1, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc1, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc1, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc1, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc1, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc1, misc_flags) == 32);
    assert!(offset_of!(D3d11Texture3dDesc1, texture_layout) == 36);
    assert!(size_of::<D3d11BufferSrv>() == 8);
    assert!(offset_of!(D3d11BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d11BufferexSrv>() == 12);
    assert!(offset_of!(D3d11BufferexSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferexSrv, num_elements) == 4);
    assert!(offset_of!(D3d11BufferexSrv, flags) == 8);
    assert!(size_of::<D3d11Tex1dSrv>() == 8);
    assert!(offset_of!(D3d11Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv>() == 8);
    assert!(offset_of!(D3d11Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv1>() == 12);
    assert!(offset_of!(D3d11Tex2dSrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dSrv1, plane_slice) == 8);
    assert!(size_of::<D3d11Tex2dArraySrv1>() == 20);
    assert!(offset_of!(D3d11Tex2dArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv1, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv1, array_size) == 12);
    assert!(offset_of!(D3d11Tex2dArraySrv1, plane_slice) == 16);
    assert!(size_of::<D3d11Tex3dSrv>() == 8);
    assert!(offset_of!(D3d11Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeSrv>() == 8);
    assert!(offset_of!(D3d11TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeArraySrv>() == 16);
    assert!(offset_of!(D3d11TexcubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11TexcubeArraySrv, first2d_array_face) == 8);
    assert!(offset_of!(D3d11TexcubeArraySrv, num_cubes) == 12);
    assert!(size_of::<D3d11Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d11ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d11ShaderResourceViewDesc1>() == 28);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d11BufferRtv>() == 8);
    assert!(offset_of!(D3d11BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d11BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d11Tex1dRtv>() == 4);
    assert!(offset_of!(D3d11Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv1>() == 8);
    assert!(offset_of!(D3d11Tex2dRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dRtv1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayRtv1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d11Tex3dRtv>() == 12);
    assert!(offset_of!(D3d11Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d11RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc1>() == 24);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, u) == 8);
    assert!(size_of::<D3d11Tex1dDsv>() == 4);
    assert!(offset_of!(D3d11Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d11DepthStencilViewDesc>() == 24);
    assert!(offset_of!(D3d11DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d11DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(D3d11DepthStencilViewDesc, u) == 12);
    assert!(size_of::<D3d11BufferUav>() == 12);
    assert!(offset_of!(D3d11BufferUav, first_element) == 0);
    assert!(offset_of!(D3d11BufferUav, num_elements) == 4);
    assert!(offset_of!(D3d11BufferUav, flags) == 8);
    assert!(size_of::<D3d11Tex1dUav>() == 4);
    assert!(offset_of!(D3d11Tex1dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav>() == 4);
    assert!(offset_of!(D3d11Tex2dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav1>() == 8);
    assert!(offset_of!(D3d11Tex2dUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dUav1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayUav1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayUav1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex3dUav>() == 12);
    assert!(offset_of!(D3d11Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dUav, w_size) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc>() == 20);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, u) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc1>() == 24);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, u) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc>() == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend_alpha) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, render_target_write_mask) == 28);
    assert!(size_of::<D3d11BlendDesc>() == 264);
    assert!(offset_of!(D3d11BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc, render_target) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc1>() == 40);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op_enable) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op_alpha) == 28);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op) == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, render_target_write_mask) == 36);
    assert!(size_of::<D3d11BlendDesc1>() == 328);
    assert!(offset_of!(D3d11BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d11DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d11DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d11DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d11DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d11RasterizerDesc>() == 40);
    assert!(offset_of!(D3d11RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d11RasterizerDesc1>() == 44);
    assert!(offset_of!(D3d11RasterizerDesc1, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc1, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc1, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc1, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc1, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc1, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc1, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc1, forced_sample_count) == 40);
    assert!(size_of::<D3d11RasterizerDesc2>() == 48);
    assert!(offset_of!(D3d11RasterizerDesc2, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc2, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc2, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc2, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc2, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc2, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc2, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc2, forced_sample_count) == 40);
    assert!(offset_of!(D3d11RasterizerDesc2, conservative_raster) == 44);
    assert!(size_of::<D3d11SamplerDesc>() == 52);
    assert!(offset_of!(D3d11SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d11SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d11SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d11SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d11SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d11SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d11SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d11SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d11SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d11SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d11InputElementDesc>() == 28);
    assert!(offset_of!(D3d11InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d11InputElementDesc, semantic_index) == 4);
    assert!(offset_of!(D3d11InputElementDesc, format) == 8);
    assert!(offset_of!(D3d11InputElementDesc, input_slot) == 12);
    assert!(offset_of!(D3d11InputElementDesc, aligned_byte_offset) == 16);
    assert!(offset_of!(D3d11InputElementDesc, input_slot_class) == 20);
    assert!(offset_of!(D3d11InputElementDesc, instance_data_step_rate) == 24);
    assert!(size_of::<D3d11SoDeclarationEntry>() == 16);
    assert!(offset_of!(D3d11SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_name) == 4);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(D3d11SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(D3d11SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(D3d11SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<D3d11ClassInstanceDesc>() == 32);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_id) == 0);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_index) == 4);
    assert!(offset_of!(D3d11ClassInstanceDesc, type_id) == 8);
    assert!(offset_of!(D3d11ClassInstanceDesc, constant_buffer) == 12);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_constant_buffer_offset) == 16);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_texture) == 20);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_sampler) == 24);
    assert!(offset_of!(D3d11ClassInstanceDesc, created) == 28);
    assert!(size_of::<D3d11QueryDesc>() == 8);
    assert!(offset_of!(D3d11QueryDesc, query) == 0);
    assert!(offset_of!(D3d11QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d11QueryDesc1>() == 12);
    assert!(offset_of!(D3d11QueryDesc1, query) == 0);
    assert!(offset_of!(D3d11QueryDesc1, misc_flags) == 4);
    assert!(offset_of!(D3d11QueryDesc1, context_type) == 8);
    assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, hs_invocations) == 64);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ds_invocations) == 72);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, cs_invocations) == 80);
    assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d11QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d11QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d11CounterDesc>() == 8);
    assert!(offset_of!(D3d11CounterDesc, counter) == 0);
    assert!(offset_of!(D3d11CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d11CounterInfo>() == 12);
    assert!(offset_of!(D3d11CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d11CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d11CounterInfo, num_detectable_parallel_units) == 8);
    assert!(size_of::<D3d11TiledResourceCoordinate>() == 16);
    assert!(offset_of!(D3d11TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(D3d11TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(D3d11TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(D3d11TiledResourceCoordinate, subresource) == 12);
    assert!(size_of::<D3d11TileRegionSize>() == 16);
    assert!(offset_of!(D3d11TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(D3d11TileRegionSize, b_use_box) == 4);
    assert!(offset_of!(D3d11TileRegionSize, width) == 8);
    assert!(offset_of!(D3d11TileRegionSize, height) == 12);
    assert!(offset_of!(D3d11TileRegionSize, depth) == 14);
    assert!(size_of::<D3d11TileShape>() == 12);
    assert!(offset_of!(D3d11TileShape, width_in_texels) == 0);
    assert!(offset_of!(D3d11TileShape, height_in_texels) == 4);
    assert!(offset_of!(D3d11TileShape, depth_in_texels) == 8);
    assert!(size_of::<D3d11SubresourceTiling>() == 12);
    assert!(offset_of!(D3d11SubresourceTiling, width_in_tiles) == 0);
    assert!(offset_of!(D3d11SubresourceTiling, height_in_tiles) == 4);
    assert!(offset_of!(D3d11SubresourceTiling, depth_in_tiles) == 6);
    assert!(offset_of!(D3d11SubresourceTiling, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11PackedMipDesc>() == 12);
    assert!(offset_of!(D3d11PackedMipDesc, num_standard_mips) == 0);
    assert!(offset_of!(D3d11PackedMipDesc, num_packed_mips) == 1);
    assert!(offset_of!(D3d11PackedMipDesc, num_tiles_for_packed_mips) == 4);
    assert!(offset_of!(D3d11PackedMipDesc, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11DrawInstancedIndirectArgs>() == 16);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, vertex_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_vertex_location) == 8);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_instance_location) == 12);
    assert!(size_of::<D3d11DrawIndexedInstancedIndirectArgs>() == 20);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, index_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_index_location) == 8);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, base_vertex_location) == 12);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_instance_location) == 16);
    assert!(size_of::<D3d11FeatureDataThreading>() == 8);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_concurrent_creates) == 0);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_command_lists) == 4);
    assert!(size_of::<D3d11FeatureDataDoubles>() == 4);
    assert!(offset_of!(D3d11FeatureDataDoubles, double_precision_float_shader_ops) == 0);
    assert!(size_of::<D3d11FeatureDataFormatSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, out_format_support) == 4);
    assert!(size_of::<D3d11FeatureDataFormatSupport2>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, out_format_support2) == 4);
    assert!(size_of::<D3d11FeatureDataD3d10XHardwareOptions>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d10XHardwareOptions, compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options>() == 56);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, output_merger_logic_op) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, uav_only_rendering_forced_sample_count) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, discard_ap_is_seen_by_driver) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, flags_for_update_and_copy_seen_by_driver) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, clear_view) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, copy_with_overlap) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_partial_update) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_offsetting) == 28);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_constant_buffer) == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_buffer_srv) == 36);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, multisample_rtv_with_forced_sample_count_one) == 40);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, sad4_shader_instructions) == 44);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_doubles_shader_instructions) == 48);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_resource_sharing) == 52);
    assert!(size_of::<D3d11FeatureDataArchitectureInfo>() == 4);
    assert!(offset_of!(D3d11FeatureDataArchitectureInfo, tile_based_deferred_renderer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options, full_non_pow2_texture_support) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9ShadowSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9ShadowSupport, supports_depth_as_texture_with_less_equal_comparison_filter) == 0);
    assert!(size_of::<D3d11FeatureDataShaderMinPrecisionSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, pixel_shader_min_precision) == 0);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, all_other_shader_stages_min_precision) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, tiled_resources_tier) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, min_max_filtering) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, clear_view_also_supports_depth_only_formats) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, map_on_default_buffers) == 12);
    assert!(size_of::<D3d11FeatureDataD3d9SimpleInstancingSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9SimpleInstancingSupport, simple_instancing_supported) == 0);
    assert!(size_of::<D3d11FeatureDataMarkerSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataMarkerSupport, profile) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, full_non_pow2_texture_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, depth_as_texture_with_less_equal_comparison_filter_supported) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, simple_instancing_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, texture_cube_face_render_target_with_non_cube_depth_stencil_supported) == 12);
    assert!(size_of::<D3d11FeatureDataD3d11Options2>() == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ps_specified_stencil_ref_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, typed_uav_load_additional_formats) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ro_vs_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, conservative_rasterization_tier) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, tiled_resources_tier) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, map_on_default_textures) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, standard_swizzle) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, unified_memory_architecture) == 28);
    assert!(size_of::<D3d11FeatureDataD3d11Options3>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options3, vp_and_rt_array_index_from_any_shader_feeding_rasterizer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options4>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options4, extended_nv12_shared_texture_supported) == 0);
    assert!(size_of::<D3d11FeatureDataGpuVirtualAddressSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_resource) == 0);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_process) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options5>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options5, shared_resource_tier) == 0);
    assert!(size_of::<D3d11FeatureDataShaderCache>() == 4);
    assert!(offset_of!(D3d11FeatureDataShaderCache, support_flags) == 0);
    assert!(size_of::<D3d10Box>() == 24);
    assert!(offset_of!(D3d10Box, left) == 0);
    assert!(offset_of!(D3d10Box, top) == 4);
    assert!(offset_of!(D3d10Box, front) == 8);
    assert!(offset_of!(D3d10Box, right) == 12);
    assert!(offset_of!(D3d10Box, bottom) == 16);
    assert!(offset_of!(D3d10Box, back) == 20);
    assert!(size_of::<D3d10Viewport>() == 24);
    assert!(offset_of!(D3d10Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d10Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d10Viewport, width) == 8);
    assert!(offset_of!(D3d10Viewport, height) == 12);
    assert!(offset_of!(D3d10Viewport, min_depth) == 16);
    assert!(offset_of!(D3d10Viewport, max_depth) == 20);
    assert!(size_of::<D3d10MappedTexture2d>() == 8);
    assert!(offset_of!(D3d10MappedTexture2d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture2d, row_pitch) == 4);
    assert!(size_of::<D3d10MappedTexture3d>() == 12);
    assert!(offset_of!(D3d10MappedTexture3d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture3d, row_pitch) == 4);
    assert!(offset_of!(D3d10MappedTexture3d, depth_pitch) == 8);
    assert!(size_of::<D3d10SubresourceData>() == 12);
    assert!(offset_of!(D3d10SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_pitch) == 4);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_slice_pitch) == 8);
    assert!(size_of::<D3d10BufferDesc>() == 20);
    assert!(offset_of!(D3d10BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d10BufferDesc, usage) == 4);
    assert!(offset_of!(D3d10BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d10BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d10BufferDesc, misc_flags) == 16);
    assert!(size_of::<D3d10Texture1dDesc>() == 32);
    assert!(offset_of!(D3d10Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d10Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d10Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d10Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d10Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d10Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d10Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d10Texture2dDesc>() == 44);
    assert!(offset_of!(D3d10Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d10Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d10Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d10Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d10Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d10Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d10Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d10Texture3dDesc>() == 36);
    assert!(offset_of!(D3d10Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d10Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d10Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d10Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d10Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d10Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d10BufferSrv>() == 8);
    assert!(offset_of!(D3d10BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d10BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dSrv>() == 8);
    assert!(offset_of!(D3d10Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex2dSrv>() == 8);
    assert!(offset_of!(D3d10Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex3dSrv>() == 8);
    assert!(offset_of!(D3d10Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeSrv>() == 8);
    assert!(offset_of!(D3d10TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeArraySrv1>() == 16);
    assert!(offset_of!(D3d10TexcubeArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d10TexcubeArraySrv1, first2d_array_face) == 8);
    assert!(offset_of!(D3d10TexcubeArraySrv1, num_cubes) == 12);
    assert!(size_of::<D3d10Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d10ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d10ShaderResourceViewDesc1>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d10BufferRtv>() == 8);
    assert!(offset_of!(D3d10BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d10BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dRtv>() == 4);
    assert!(offset_of!(D3d10Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d10Tex3dRtv>() == 12);
    assert!(offset_of!(D3d10Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d10Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d10RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d10RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d10RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d10Tex1dDsv>() == 4);
    assert!(offset_of!(D3d10Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d10DepthStencilViewDesc>() == 20);
    assert!(offset_of!(D3d10DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d10DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10DepthStencilViewDesc, u) == 8);
    assert!(size_of::<D3d10BlendDesc>() == 68);
    assert!(offset_of!(D3d10BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc, blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc, src_blend) == 36);
    assert!(offset_of!(D3d10BlendDesc, dest_blend) == 40);
    assert!(offset_of!(D3d10BlendDesc, blend_op) == 44);
    assert!(offset_of!(D3d10BlendDesc, src_blend_alpha) == 48);
    assert!(offset_of!(D3d10BlendDesc, dest_blend_alpha) == 52);
    assert!(offset_of!(D3d10BlendDesc, blend_op_alpha) == 56);
    assert!(offset_of!(D3d10BlendDesc, render_target_write_mask) == 60);
    assert!(size_of::<D3d10RenderTargetBlendDesc1>() == 32);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend) == 4);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend) == 8);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op) == 12);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend_alpha) == 16);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op_alpha) == 24);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, render_target_write_mask) == 28);
    assert!(size_of::<D3d10BlendDesc1>() == 264);
    assert!(offset_of!(D3d10BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d10DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d10DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d10DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d10DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d10RasterizerDesc>() == 40);
    assert!(offset_of!(D3d10RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d10RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d10RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d10RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d10RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d10RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d10RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d10RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d10SamplerDesc>() == 52);
    assert!(offset_of!(D3d10SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d10SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d10SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d10SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d10SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d10SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d10SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d10SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d10SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d10SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d10InputElementDesc>() == 28);
    assert!(offset_of!(D3d10InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d10InputElementDesc, semantic_index) == 4);
    assert!(offset_of!(D3d10InputElementDesc, format) == 8);
    assert!(offset_of!(D3d10InputElementDesc, input_slot) == 12);
    assert!(offset_of!(D3d10InputElementDesc, aligned_byte_offset) == 16);
    assert!(offset_of!(D3d10InputElementDesc, input_slot_class) == 20);
    assert!(offset_of!(D3d10InputElementDesc, instance_data_step_rate) == 24);
    assert!(size_of::<D3d10SoDeclarationEntry>() == 12);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_name) == 0);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_index) == 4);
    assert!(offset_of!(D3d10SoDeclarationEntry, start_component) == 8);
    assert!(offset_of!(D3d10SoDeclarationEntry, component_count) == 9);
    assert!(offset_of!(D3d10SoDeclarationEntry, output_slot) == 10);
    assert!(size_of::<D3d10QueryDesc>() == 8);
    assert!(offset_of!(D3d10QueryDesc, query) == 0);
    assert!(offset_of!(D3d10QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d10QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d10QueryDataPipelineStatistics>() == 64);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(size_of::<D3d10QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d10QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d10QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d10CounterDesc>() == 8);
    assert!(offset_of!(D3d10CounterDesc, counter) == 0);
    assert!(offset_of!(D3d10CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d10CounterInfo>() == 12);
    assert!(offset_of!(D3d10CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d10CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d10CounterInfo, num_detectable_parallel_units) == 8);
};

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(size_of::<Luid>() == 8);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<Rect>() == 16);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Point>() == 8);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
    assert!(size_of::<SecurityAttributes>() == 24);
    assert!(offset_of!(SecurityAttributes, n_length) == 0);
    assert!(offset_of!(SecurityAttributes, lp_security_descriptor) == 8);
    assert!(offset_of!(SecurityAttributes, b_inherit_handle) == 16);
    assert!(size_of::<DxgiRational>() == 8);
    assert!(offset_of!(DxgiRational, numerator) == 0);
    assert!(offset_of!(DxgiRational, denominator) == 4);
    assert!(size_of::<DxgiSampleDesc>() == 8);
    assert!(offset_of!(DxgiSampleDesc, count) == 0);
    assert!(offset_of!(DxgiSampleDesc, quality) == 4);
    assert!(size_of::<DxgiRgb>() == 12);
    assert!(offset_of!(DxgiRgb, red) == 0);
    assert!(offset_of!(DxgiRgb, green) == 4);
    assert!(offset_of!(DxgiRgb, blue) == 8);
    assert!(size_of::<DxgiRgba>() == 16);
    assert!(offset_of!(DxgiRgba, r) == 0);
    assert!(offset_of!(DxgiRgba, g) == 4);
    assert!(offset_of!(DxgiRgba, b) == 8);
    assert!(offset_of!(DxgiRgba, a) == 12);
    assert!(size_of::<DxgiGammaControl>() == 12324);
    assert!(offset_of!(DxgiGammaControl, scale) == 0);
    assert!(offset_of!(DxgiGammaControl, offset) == 12);
    assert!(offset_of!(DxgiGammaControl, gamma_curve) == 24);
    assert!(size_of::<DxgiGammaControlCapabilities>() == 4116);
    assert!(offset_of!(DxgiGammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(DxgiGammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(DxgiGammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(DxgiGammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(DxgiGammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<DxgiModeDesc>() == 28);
    assert!(offset_of!(DxgiModeDesc, width) == 0);
    assert!(offset_of!(DxgiModeDesc, height) == 4);
    assert!(offset_of!(DxgiModeDesc, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc, format) == 16);
    assert!(offset_of!(DxgiModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc, scaling) == 24);
    assert!(size_of::<DxgiModeDesc1>() == 32);
    assert!(offset_of!(DxgiModeDesc1, width) == 0);
    assert!(offset_of!(DxgiModeDesc1, height) == 4);
    assert!(offset_of!(DxgiModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(DxgiModeDesc1, format) == 16);
    assert!(offset_of!(DxgiModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(DxgiModeDesc1, scaling) == 24);
    assert!(offset_of!(DxgiModeDesc1, stereo) == 28);
    assert!(size_of::<DxgiSurfaceDesc>() == 20);
    assert!(offset_of!(DxgiSurfaceDesc, width) == 0);
    assert!(offset_of!(DxgiSurfaceDesc, height) == 4);
    assert!(offset_of!(DxgiSurfaceDesc, format) == 8);
    assert!(offset_of!(DxgiSurfaceDesc, sample_desc) == 12);
    assert!(size_of::<DxgiMappedRect>() == 16);
    assert!(offset_of!(DxgiMappedRect, pitch) == 0);
    assert!(offset_of!(DxgiMappedRect, p_bits) == 8);
    assert!(size_of::<DxgiSharedResource>() == 8);
    assert!(offset_of!(DxgiSharedResource, handle) == 0);
    assert!(size_of::<DxgiFrameStatistics>() == 32);
    assert!(offset_of!(DxgiFrameStatistics, present_count) == 0);
    assert!(offset_of!(DxgiFrameStatistics, present_refresh_count) == 4);
    assert!(offset_of!(DxgiFrameStatistics, sync_refresh_count) == 8);
    assert!(offset_of!(DxgiFrameStatistics, sync_qpc_time) == 16);
    assert!(offset_of!(DxgiFrameStatistics, sync_gpu_time) == 24);
    assert!(size_of::<DxgiSwapChainDesc>() == 72);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(DxgiSwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(DxgiSwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(DxgiSwapChainDesc, output_window) == 48);
    assert!(offset_of!(DxgiSwapChainDesc, windowed) == 56);
    assert!(offset_of!(DxgiSwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(DxgiSwapChainDesc, flags) == 64);
    assert!(size_of::<DxgiSwapChainDesc1>() == 48);
    assert!(offset_of!(DxgiSwapChainDesc1, width) == 0);
    assert!(offset_of!(DxgiSwapChainDesc1, height) == 4);
    assert!(offset_of!(DxgiSwapChainDesc1, format) == 8);
    assert!(offset_of!(DxgiSwapChainDesc1, stereo) == 12);
    assert!(offset_of!(DxgiSwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(DxgiSwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(DxgiSwapChainDesc1, scaling) == 32);
    assert!(offset_of!(DxgiSwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(DxgiSwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(DxgiSwapChainDesc1, flags) == 44);
    assert!(size_of::<DxgiSwapChainFullscreenDesc>() == 20);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(DxgiSwapChainFullscreenDesc, windowed) == 16);
    assert!(size_of::<DxgiPresentParameters>() == 32);
    assert!(offset_of!(DxgiPresentParameters, dirty_rects_count) == 0);
    assert!(offset_of!(DxgiPresentParameters, p_dirty_rects) == 8);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_rect) == 16);
    assert!(offset_of!(DxgiPresentParameters, p_scroll_offset) == 24);
    assert!(size_of::<DxgiMatrix3x2F>() == 24);
    assert!(offset_of!(DxgiMatrix3x2F, m_11) == 0);
    assert!(offset_of!(DxgiMatrix3x2F, m_12) == 4);
    assert!(offset_of!(DxgiMatrix3x2F, m_21) == 8);
    assert!(offset_of!(DxgiMatrix3x2F, m_22) == 12);
    assert!(offset_of!(DxgiMatrix3x2F, m_31) == 16);
    assert!(offset_of!(DxgiMatrix3x2F, m_32) == 20);
    assert!(size_of::<DxgiHdrMetadataHdr10>() == 28);
    assert!(offset_of!(DxgiHdrMetadataHdr10, red_primary) == 0);
    assert!(offset_of!(DxgiHdrMetadataHdr10, green_primary) == 4);
    assert!(offset_of!(DxgiHdrMetadataHdr10, blue_primary) == 8);
    assert!(offset_of!(DxgiHdrMetadataHdr10, white_point) == 12);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_mastering_luminance) == 16);
    assert!(offset_of!(DxgiHdrMetadataHdr10, min_mastering_luminance) == 20);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_content_light_level) == 24);
    assert!(offset_of!(DxgiHdrMetadataHdr10, max_frame_average_light_level) == 26);
    assert!(size_of::<D3d11Box>() == 24);
    assert!(offset_of!(D3d11Box, left) == 0);
    assert!(offset_of!(D3d11Box, top) == 4);
    assert!(offset_of!(D3d11Box, front) == 8);
    assert!(offset_of!(D3d11Box, right) == 12);
    assert!(offset_of!(D3d11Box, bottom) == 16);
    assert!(offset_of!(D3d11Box, back) == 20);
    assert!(size_of::<D3d11Viewport>() == 24);
    assert!(offset_of!(D3d11Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d11Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d11Viewport, width) == 8);
    assert!(offset_of!(D3d11Viewport, height) == 12);
    assert!(offset_of!(D3d11Viewport, min_depth) == 16);
    assert!(offset_of!(D3d11Viewport, max_depth) == 20);
    assert!(size_of::<D3d11MappedSubresource>() == 16);
    assert!(offset_of!(D3d11MappedSubresource, p_data) == 0);
    assert!(offset_of!(D3d11MappedSubresource, row_pitch) == 8);
    assert!(offset_of!(D3d11MappedSubresource, depth_pitch) == 12);
    assert!(size_of::<D3d11SubresourceData>() == 16);
    assert!(offset_of!(D3d11SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d11SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d11BufferDesc>() == 24);
    assert!(offset_of!(D3d11BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d11BufferDesc, usage) == 4);
    assert!(offset_of!(D3d11BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d11BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d11BufferDesc, misc_flags) == 16);
    assert!(offset_of!(D3d11BufferDesc, structure_byte_stride) == 20);
    assert!(size_of::<D3d11Texture1dDesc>() == 32);
    assert!(offset_of!(D3d11Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d11Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d11Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d11Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d11Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d11Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d11Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d11Texture2dDesc>() == 44);
    assert!(offset_of!(D3d11Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d11Texture2dDesc1>() == 48);
    assert!(offset_of!(D3d11Texture2dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture2dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture2dDesc1, mip_levels) == 8);
    assert!(offset_of!(D3d11Texture2dDesc1, array_size) == 12);
    assert!(offset_of!(D3d11Texture2dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture2dDesc1, sample_desc) == 20);
    assert!(offset_of!(D3d11Texture2dDesc1, usage) == 28);
    assert!(offset_of!(D3d11Texture2dDesc1, bind_flags) == 32);
    assert!(offset_of!(D3d11Texture2dDesc1, cpu_access_flags) == 36);
    assert!(offset_of!(D3d11Texture2dDesc1, misc_flags) == 40);
    assert!(offset_of!(D3d11Texture2dDesc1, texture_layout) == 44);
    assert!(size_of::<D3d11Texture3dDesc>() == 36);
    assert!(offset_of!(D3d11Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d11Texture3dDesc1>() == 40);
    assert!(offset_of!(D3d11Texture3dDesc1, width) == 0);
    assert!(offset_of!(D3d11Texture3dDesc1, height) == 4);
    assert!(offset_of!(D3d11Texture3dDesc1, depth) == 8);
    assert!(offset_of!(D3d11Texture3dDesc1, mip_levels) == 12);
    assert!(offset_of!(D3d11Texture3dDesc1, format) == 16);
    assert!(offset_of!(D3d11Texture3dDesc1, usage) == 20);
    assert!(offset_of!(D3d11Texture3dDesc1, bind_flags) == 24);
    assert!(offset_of!(D3d11Texture3dDesc1, cpu_access_flags) == 28);
    assert!(offset_of!(D3d11Texture3dDesc1, misc_flags) == 32);
    assert!(offset_of!(D3d11Texture3dDesc1, texture_layout) == 36);
    assert!(size_of::<D3d11BufferSrv>() == 8);
    assert!(offset_of!(D3d11BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d11BufferexSrv>() == 12);
    assert!(offset_of!(D3d11BufferexSrv, first_element) == 0);
    assert!(offset_of!(D3d11BufferexSrv, num_elements) == 4);
    assert!(offset_of!(D3d11BufferexSrv, flags) == 8);
    assert!(size_of::<D3d11Tex1dSrv>() == 8);
    assert!(offset_of!(D3d11Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv>() == 8);
    assert!(offset_of!(D3d11Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d11Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d11Tex2dSrv1>() == 12);
    assert!(offset_of!(D3d11Tex2dSrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dSrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dSrv1, plane_slice) == 8);
    assert!(size_of::<D3d11Tex2dArraySrv1>() == 20);
    assert!(offset_of!(D3d11Tex2dArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex2dArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d11Tex2dArraySrv1, first_array_slice) == 8);
    assert!(offset_of!(D3d11Tex2dArraySrv1, array_size) == 12);
    assert!(offset_of!(D3d11Tex2dArraySrv1, plane_slice) == 16);
    assert!(size_of::<D3d11Tex3dSrv>() == 8);
    assert!(offset_of!(D3d11Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeSrv>() == 8);
    assert!(offset_of!(D3d11TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d11TexcubeArraySrv>() == 16);
    assert!(offset_of!(D3d11TexcubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d11TexcubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d11TexcubeArraySrv, first2d_array_face) == 8);
    assert!(offset_of!(D3d11TexcubeArraySrv, num_cubes) == 12);
    assert!(size_of::<D3d11Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d11ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d11ShaderResourceViewDesc1>() == 28);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d11BufferRtv>() == 8);
    assert!(offset_of!(D3d11BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d11BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d11Tex1dRtv>() == 4);
    assert!(offset_of!(D3d11Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dRtv1>() == 8);
    assert!(offset_of!(D3d11Tex2dRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dRtv1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayRtv1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayRtv1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d11Tex3dRtv>() == 12);
    assert!(offset_of!(D3d11Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d11RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d11RenderTargetViewDesc1>() == 24);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, format) == 0);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11RenderTargetViewDesc1, u) == 8);
    assert!(size_of::<D3d11Tex1dDsv>() == 4);
    assert!(offset_of!(D3d11Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d11Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d11Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d11Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d11Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d11DepthStencilViewDesc>() == 24);
    assert!(offset_of!(D3d11DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d11DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(D3d11DepthStencilViewDesc, u) == 12);
    assert!(size_of::<D3d11BufferUav>() == 12);
    assert!(offset_of!(D3d11BufferUav, first_element) == 0);
    assert!(offset_of!(D3d11BufferUav, num_elements) == 4);
    assert!(offset_of!(D3d11BufferUav, flags) == 8);
    assert!(size_of::<D3d11Tex1dUav>() == 4);
    assert!(offset_of!(D3d11Tex1dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex1dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex1dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav>() == 4);
    assert!(offset_of!(D3d11Tex2dUav, mip_slice) == 0);
    assert!(size_of::<D3d11Tex2dArrayUav>() == 12);
    assert!(offset_of!(D3d11Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav, array_size) == 8);
    assert!(size_of::<D3d11Tex2dUav1>() == 8);
    assert!(offset_of!(D3d11Tex2dUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dUav1, plane_slice) == 4);
    assert!(size_of::<D3d11Tex2dArrayUav1>() == 16);
    assert!(offset_of!(D3d11Tex2dArrayUav1, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex2dArrayUav1, first_array_slice) == 4);
    assert!(offset_of!(D3d11Tex2dArrayUav1, array_size) == 8);
    assert!(offset_of!(D3d11Tex2dArrayUav1, plane_slice) == 12);
    assert!(size_of::<D3d11Tex3dUav>() == 12);
    assert!(offset_of!(D3d11Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(D3d11Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(D3d11Tex3dUav, w_size) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc>() == 20);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc, u) == 8);
    assert!(size_of::<D3d11UnorderedAccessViewDesc1>() == 24);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, format) == 0);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d11UnorderedAccessViewDesc1, u) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc>() == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, src_blend_alpha) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, blend_op_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc, render_target_write_mask) == 28);
    assert!(size_of::<D3d11BlendDesc>() == 264);
    assert!(offset_of!(D3d11BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc, render_target) == 8);
    assert!(size_of::<D3d11RenderTargetBlendDesc1>() == 40);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op_enable) == 4);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend) == 8);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend) == 12);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op) == 16);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, src_blend_alpha) == 20);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, dest_blend_alpha) == 24);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, blend_op_alpha) == 28);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, logic_op) == 32);
    assert!(offset_of!(D3d11RenderTargetBlendDesc1, render_target_write_mask) == 36);
    assert!(size_of::<D3d11BlendDesc1>() == 328);
    assert!(offset_of!(D3d11BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d11BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d11BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d11DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d11DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d11DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d11DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d11DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d11DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d11DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d11RasterizerDesc>() == 40);
    assert!(offset_of!(D3d11RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d11RasterizerDesc1>() == 44);
    assert!(offset_of!(D3d11RasterizerDesc1, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc1, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc1, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc1, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc1, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc1, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc1, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc1, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc1, forced_sample_count) == 40);
    assert!(size_of::<D3d11RasterizerDesc2>() == 48);
    assert!(offset_of!(D3d11RasterizerDesc2, fill_mode) == 0);
    assert!(offset_of!(D3d11RasterizerDesc2, cull_mode) == 4);
    assert!(offset_of!(D3d11RasterizerDesc2, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias) == 12);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d11RasterizerDesc2, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d11RasterizerDesc2, depth_clip_enable) == 24);
    assert!(offset_of!(D3d11RasterizerDesc2, scissor_enable) == 28);
    assert!(offset_of!(D3d11RasterizerDesc2, multisample_enable) == 32);
    assert!(offset_of!(D3d11RasterizerDesc2, antialiased_line_enable) == 36);
    assert!(offset_of!(D3d11RasterizerDesc2, forced_sample_count) == 40);
    assert!(offset_of!(D3d11RasterizerDesc2, conservative_raster) == 44);
    assert!(size_of::<D3d11SamplerDesc>() == 52);
    assert!(offset_of!(D3d11SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d11SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d11SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d11SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d11SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d11SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d11SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d11SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d11SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d11SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d11InputElementDesc>() == 32);
    assert!(offset_of!(D3d11InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d11InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d11InputElementDesc, format) == 12);
    assert!(offset_of!(D3d11InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d11InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d11InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d11InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d11SoDeclarationEntry>() == 24);
    assert!(offset_of!(D3d11SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_name) == 8);
    assert!(offset_of!(D3d11SoDeclarationEntry, semantic_index) == 16);
    assert!(offset_of!(D3d11SoDeclarationEntry, start_component) == 20);
    assert!(offset_of!(D3d11SoDeclarationEntry, component_count) == 21);
    assert!(offset_of!(D3d11SoDeclarationEntry, output_slot) == 22);
    assert!(size_of::<D3d11ClassInstanceDesc>() == 32);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_id) == 0);
    assert!(offset_of!(D3d11ClassInstanceDesc, instance_index) == 4);
    assert!(offset_of!(D3d11ClassInstanceDesc, type_id) == 8);
    assert!(offset_of!(D3d11ClassInstanceDesc, constant_buffer) == 12);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_constant_buffer_offset) == 16);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_texture) == 20);
    assert!(offset_of!(D3d11ClassInstanceDesc, base_sampler) == 24);
    assert!(offset_of!(D3d11ClassInstanceDesc, created) == 28);
    assert!(size_of::<D3d11QueryDesc>() == 8);
    assert!(offset_of!(D3d11QueryDesc, query) == 0);
    assert!(offset_of!(D3d11QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d11QueryDesc1>() == 12);
    assert!(offset_of!(D3d11QueryDesc1, query) == 0);
    assert!(offset_of!(D3d11QueryDesc1, misc_flags) == 4);
    assert!(offset_of!(D3d11QueryDesc1, context_type) == 8);
    assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d11QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, hs_invocations) == 64);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, ds_invocations) == 72);
    assert!(offset_of!(D3d11QueryDataPipelineStatistics, cs_invocations) == 80);
    assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d11QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d11QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d11CounterDesc>() == 8);
    assert!(offset_of!(D3d11CounterDesc, counter) == 0);
    assert!(offset_of!(D3d11CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d11CounterInfo>() == 12);
    assert!(offset_of!(D3d11CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d11CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d11CounterInfo, num_detectable_parallel_units) == 8);
    assert!(size_of::<D3d11TiledResourceCoordinate>() == 16);
    assert!(offset_of!(D3d11TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(D3d11TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(D3d11TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(D3d11TiledResourceCoordinate, subresource) == 12);
    assert!(size_of::<D3d11TileRegionSize>() == 16);
    assert!(offset_of!(D3d11TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(D3d11TileRegionSize, b_use_box) == 4);
    assert!(offset_of!(D3d11TileRegionSize, width) == 8);
    assert!(offset_of!(D3d11TileRegionSize, height) == 12);
    assert!(offset_of!(D3d11TileRegionSize, depth) == 14);
    assert!(size_of::<D3d11TileShape>() == 12);
    assert!(offset_of!(D3d11TileShape, width_in_texels) == 0);
    assert!(offset_of!(D3d11TileShape, height_in_texels) == 4);
    assert!(offset_of!(D3d11TileShape, depth_in_texels) == 8);
    assert!(size_of::<D3d11SubresourceTiling>() == 12);
    assert!(offset_of!(D3d11SubresourceTiling, width_in_tiles) == 0);
    assert!(offset_of!(D3d11SubresourceTiling, height_in_tiles) == 4);
    assert!(offset_of!(D3d11SubresourceTiling, depth_in_tiles) == 6);
    assert!(offset_of!(D3d11SubresourceTiling, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11PackedMipDesc>() == 12);
    assert!(offset_of!(D3d11PackedMipDesc, num_standard_mips) == 0);
    assert!(offset_of!(D3d11PackedMipDesc, num_packed_mips) == 1);
    assert!(offset_of!(D3d11PackedMipDesc, num_tiles_for_packed_mips) == 4);
    assert!(offset_of!(D3d11PackedMipDesc, start_tile_index_in_overall_resource) == 8);
    assert!(size_of::<D3d11DrawInstancedIndirectArgs>() == 16);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, vertex_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_vertex_location) == 8);
    assert!(offset_of!(D3d11DrawInstancedIndirectArgs, start_instance_location) == 12);
    assert!(size_of::<D3d11DrawIndexedInstancedIndirectArgs>() == 20);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, index_count_per_instance) == 0);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, instance_count) == 4);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_index_location) == 8);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, base_vertex_location) == 12);
    assert!(offset_of!(D3d11DrawIndexedInstancedIndirectArgs, start_instance_location) == 16);
    assert!(size_of::<D3d11FeatureDataThreading>() == 8);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_concurrent_creates) == 0);
    assert!(offset_of!(D3d11FeatureDataThreading, driver_command_lists) == 4);
    assert!(size_of::<D3d11FeatureDataDoubles>() == 4);
    assert!(offset_of!(D3d11FeatureDataDoubles, double_precision_float_shader_ops) == 0);
    assert!(size_of::<D3d11FeatureDataFormatSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport, out_format_support) == 4);
    assert!(size_of::<D3d11FeatureDataFormatSupport2>() == 8);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, in_format) == 0);
    assert!(offset_of!(D3d11FeatureDataFormatSupport2, out_format_support2) == 4);
    assert!(size_of::<D3d11FeatureDataD3d10XHardwareOptions>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d10XHardwareOptions, compute_shaders_plus_raw_and_structured_buffers_via_shader_4_x) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options>() == 56);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, output_merger_logic_op) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, uav_only_rendering_forced_sample_count) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, discard_ap_is_seen_by_driver) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, flags_for_update_and_copy_seen_by_driver) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, clear_view) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, copy_with_overlap) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_partial_update) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, constant_buffer_offsetting) == 28);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_constant_buffer) == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, map_no_overwrite_on_dynamic_buffer_srv) == 36);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, multisample_rtv_with_forced_sample_count_one) == 40);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, sad4_shader_instructions) == 44);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_doubles_shader_instructions) == 48);
    assert!(offset_of!(D3d11FeatureDataD3d11Options, extended_resource_sharing) == 52);
    assert!(size_of::<D3d11FeatureDataArchitectureInfo>() == 4);
    assert!(offset_of!(D3d11FeatureDataArchitectureInfo, tile_based_deferred_renderer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options, full_non_pow2_texture_support) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9ShadowSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9ShadowSupport, supports_depth_as_texture_with_less_equal_comparison_filter) == 0);
    assert!(size_of::<D3d11FeatureDataShaderMinPrecisionSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, pixel_shader_min_precision) == 0);
    assert!(offset_of!(D3d11FeatureDataShaderMinPrecisionSupport, all_other_shader_stages_min_precision) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, tiled_resources_tier) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, min_max_filtering) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, clear_view_also_supports_depth_only_formats) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options1, map_on_default_buffers) == 12);
    assert!(size_of::<D3d11FeatureDataD3d9SimpleInstancingSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9SimpleInstancingSupport, simple_instancing_supported) == 0);
    assert!(size_of::<D3d11FeatureDataMarkerSupport>() == 4);
    assert!(offset_of!(D3d11FeatureDataMarkerSupport, profile) == 0);
    assert!(size_of::<D3d11FeatureDataD3d9Options1>() == 16);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, full_non_pow2_texture_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, depth_as_texture_with_less_equal_comparison_filter_supported) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, simple_instancing_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d9Options1, texture_cube_face_render_target_with_non_cube_depth_stencil_supported) == 12);
    assert!(size_of::<D3d11FeatureDataD3d11Options2>() == 32);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ps_specified_stencil_ref_supported) == 0);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, typed_uav_load_additional_formats) == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, ro_vs_supported) == 8);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, conservative_rasterization_tier) == 12);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, tiled_resources_tier) == 16);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, map_on_default_textures) == 20);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, standard_swizzle) == 24);
    assert!(offset_of!(D3d11FeatureDataD3d11Options2, unified_memory_architecture) == 28);
    assert!(size_of::<D3d11FeatureDataD3d11Options3>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options3, vp_and_rt_array_index_from_any_shader_feeding_rasterizer) == 0);
    assert!(size_of::<D3d11FeatureDataD3d11Options4>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options4, extended_nv12_shared_texture_supported) == 0);
    assert!(size_of::<D3d11FeatureDataGpuVirtualAddressSupport>() == 8);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_resource) == 0);
    assert!(offset_of!(D3d11FeatureDataGpuVirtualAddressSupport, max_gpu_virtual_address_bits_per_process) == 4);
    assert!(size_of::<D3d11FeatureDataD3d11Options5>() == 4);
    assert!(offset_of!(D3d11FeatureDataD3d11Options5, shared_resource_tier) == 0);
    assert!(size_of::<D3d11FeatureDataShaderCache>() == 4);
    assert!(offset_of!(D3d11FeatureDataShaderCache, support_flags) == 0);
    assert!(size_of::<D3d10Box>() == 24);
    assert!(offset_of!(D3d10Box, left) == 0);
    assert!(offset_of!(D3d10Box, top) == 4);
    assert!(offset_of!(D3d10Box, front) == 8);
    assert!(offset_of!(D3d10Box, right) == 12);
    assert!(offset_of!(D3d10Box, bottom) == 16);
    assert!(offset_of!(D3d10Box, back) == 20);
    assert!(size_of::<D3d10Viewport>() == 24);
    assert!(offset_of!(D3d10Viewport, top_left_x) == 0);
    assert!(offset_of!(D3d10Viewport, top_left_y) == 4);
    assert!(offset_of!(D3d10Viewport, width) == 8);
    assert!(offset_of!(D3d10Viewport, height) == 12);
    assert!(offset_of!(D3d10Viewport, min_depth) == 16);
    assert!(offset_of!(D3d10Viewport, max_depth) == 20);
    assert!(size_of::<D3d10MappedTexture2d>() == 16);
    assert!(offset_of!(D3d10MappedTexture2d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture2d, row_pitch) == 8);
    assert!(size_of::<D3d10MappedTexture3d>() == 16);
    assert!(offset_of!(D3d10MappedTexture3d, p_data) == 0);
    assert!(offset_of!(D3d10MappedTexture3d, row_pitch) == 8);
    assert!(offset_of!(D3d10MappedTexture3d, depth_pitch) == 12);
    assert!(size_of::<D3d10SubresourceData>() == 16);
    assert!(offset_of!(D3d10SubresourceData, p_sys_mem) == 0);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_pitch) == 8);
    assert!(offset_of!(D3d10SubresourceData, sys_mem_slice_pitch) == 12);
    assert!(size_of::<D3d10BufferDesc>() == 20);
    assert!(offset_of!(D3d10BufferDesc, byte_width) == 0);
    assert!(offset_of!(D3d10BufferDesc, usage) == 4);
    assert!(offset_of!(D3d10BufferDesc, bind_flags) == 8);
    assert!(offset_of!(D3d10BufferDesc, cpu_access_flags) == 12);
    assert!(offset_of!(D3d10BufferDesc, misc_flags) == 16);
    assert!(size_of::<D3d10Texture1dDesc>() == 32);
    assert!(offset_of!(D3d10Texture1dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture1dDesc, mip_levels) == 4);
    assert!(offset_of!(D3d10Texture1dDesc, array_size) == 8);
    assert!(offset_of!(D3d10Texture1dDesc, format) == 12);
    assert!(offset_of!(D3d10Texture1dDesc, usage) == 16);
    assert!(offset_of!(D3d10Texture1dDesc, bind_flags) == 20);
    assert!(offset_of!(D3d10Texture1dDesc, cpu_access_flags) == 24);
    assert!(offset_of!(D3d10Texture1dDesc, misc_flags) == 28);
    assert!(size_of::<D3d10Texture2dDesc>() == 44);
    assert!(offset_of!(D3d10Texture2dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture2dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture2dDesc, mip_levels) == 8);
    assert!(offset_of!(D3d10Texture2dDesc, array_size) == 12);
    assert!(offset_of!(D3d10Texture2dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture2dDesc, sample_desc) == 20);
    assert!(offset_of!(D3d10Texture2dDesc, usage) == 28);
    assert!(offset_of!(D3d10Texture2dDesc, bind_flags) == 32);
    assert!(offset_of!(D3d10Texture2dDesc, cpu_access_flags) == 36);
    assert!(offset_of!(D3d10Texture2dDesc, misc_flags) == 40);
    assert!(size_of::<D3d10Texture3dDesc>() == 36);
    assert!(offset_of!(D3d10Texture3dDesc, width) == 0);
    assert!(offset_of!(D3d10Texture3dDesc, height) == 4);
    assert!(offset_of!(D3d10Texture3dDesc, depth) == 8);
    assert!(offset_of!(D3d10Texture3dDesc, mip_levels) == 12);
    assert!(offset_of!(D3d10Texture3dDesc, format) == 16);
    assert!(offset_of!(D3d10Texture3dDesc, usage) == 20);
    assert!(offset_of!(D3d10Texture3dDesc, bind_flags) == 24);
    assert!(offset_of!(D3d10Texture3dDesc, cpu_access_flags) == 28);
    assert!(offset_of!(D3d10Texture3dDesc, misc_flags) == 32);
    assert!(size_of::<D3d10BufferSrv>() == 8);
    assert!(offset_of!(D3d10BufferSrv, first_element) == 0);
    assert!(offset_of!(D3d10BufferSrv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dSrv>() == 8);
    assert!(offset_of!(D3d10Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex1dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex1dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex2dSrv>() == 8);
    assert!(offset_of!(D3d10Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10Tex2dArraySrv>() == 16);
    assert!(offset_of!(D3d10Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(D3d10Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(D3d10Tex2dArraySrv, array_size) == 12);
    assert!(size_of::<D3d10Tex3dSrv>() == 8);
    assert!(offset_of!(D3d10Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10Tex3dSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeSrv>() == 8);
    assert!(offset_of!(D3d10TexcubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeSrv, mip_levels) == 4);
    assert!(size_of::<D3d10TexcubeArraySrv1>() == 16);
    assert!(offset_of!(D3d10TexcubeArraySrv1, most_detailed_mip) == 0);
    assert!(offset_of!(D3d10TexcubeArraySrv1, mip_levels) == 4);
    assert!(offset_of!(D3d10TexcubeArraySrv1, first2d_array_face) == 8);
    assert!(offset_of!(D3d10TexcubeArraySrv1, num_cubes) == 12);
    assert!(size_of::<D3d10Tex2dmsSrv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsSrv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArraySrv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArraySrv, array_size) == 4);
    assert!(size_of::<D3d10ShaderResourceViewDesc>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc, u) == 8);
    assert!(size_of::<D3d10ShaderResourceViewDesc1>() == 24);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, format) == 0);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, view_dimension) == 4);
    assert!(offset_of!(D3d10ShaderResourceViewDesc1, u) == 8);
    assert!(size_of::<D3d10BufferRtv>() == 8);
    assert!(offset_of!(D3d10BufferRtv, first_element) == 0);
    assert!(offset_of!(D3d10BufferRtv, num_elements) == 4);
    assert!(size_of::<D3d10Tex1dRtv>() == 4);
    assert!(offset_of!(D3d10Tex1dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dRtv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayRtv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayRtv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsRtv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsRtv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayRtv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayRtv, array_size) == 4);
    assert!(size_of::<D3d10Tex3dRtv>() == 12);
    assert!(offset_of!(D3d10Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(D3d10Tex3dRtv, w_size) == 8);
    assert!(size_of::<D3d10RenderTargetViewDesc>() == 20);
    assert!(offset_of!(D3d10RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(D3d10RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10RenderTargetViewDesc, u) == 8);
    assert!(size_of::<D3d10Tex1dDsv>() == 4);
    assert!(offset_of!(D3d10Tex1dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex1dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex1dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dDsv, mip_slice) == 0);
    assert!(size_of::<D3d10Tex2dArrayDsv>() == 12);
    assert!(offset_of!(D3d10Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(D3d10Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(D3d10Tex2dArrayDsv, array_size) == 8);
    assert!(size_of::<D3d10Tex2dmsDsv>() == 4);
    assert!(offset_of!(D3d10Tex2dmsDsv, unused_field_nothing_to_define) == 0);
    assert!(size_of::<D3d10Tex2dmsArrayDsv>() == 8);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(D3d10Tex2dmsArrayDsv, array_size) == 4);
    assert!(size_of::<D3d10DepthStencilViewDesc>() == 20);
    assert!(offset_of!(D3d10DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(D3d10DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(D3d10DepthStencilViewDesc, u) == 8);
    assert!(size_of::<D3d10BlendDesc>() == 68);
    assert!(offset_of!(D3d10BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc, blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc, src_blend) == 36);
    assert!(offset_of!(D3d10BlendDesc, dest_blend) == 40);
    assert!(offset_of!(D3d10BlendDesc, blend_op) == 44);
    assert!(offset_of!(D3d10BlendDesc, src_blend_alpha) == 48);
    assert!(offset_of!(D3d10BlendDesc, dest_blend_alpha) == 52);
    assert!(offset_of!(D3d10BlendDesc, blend_op_alpha) == 56);
    assert!(offset_of!(D3d10BlendDesc, render_target_write_mask) == 60);
    assert!(size_of::<D3d10RenderTargetBlendDesc1>() == 32);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_enable) == 0);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend) == 4);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend) == 8);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op) == 12);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, src_blend_alpha) == 16);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, dest_blend_alpha) == 20);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, blend_op_alpha) == 24);
    assert!(offset_of!(D3d10RenderTargetBlendDesc1, render_target_write_mask) == 28);
    assert!(size_of::<D3d10BlendDesc1>() == 264);
    assert!(offset_of!(D3d10BlendDesc1, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(D3d10BlendDesc1, independent_blend_enable) == 4);
    assert!(offset_of!(D3d10BlendDesc1, render_target) == 8);
    assert!(size_of::<D3d10DepthStencilopDesc>() == 16);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_fail_op) == 0);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_pass_op) == 8);
    assert!(offset_of!(D3d10DepthStencilopDesc, stencil_func) == 12);
    assert!(size_of::<D3d10DepthStencilDesc>() == 52);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(D3d10DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(D3d10DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(D3d10DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(D3d10DepthStencilDesc, back_face) == 36);
    assert!(size_of::<D3d10RasterizerDesc>() == 40);
    assert!(offset_of!(D3d10RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(D3d10RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(D3d10RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(D3d10RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(D3d10RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(D3d10RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(D3d10RasterizerDesc, scissor_enable) == 28);
    assert!(offset_of!(D3d10RasterizerDesc, multisample_enable) == 32);
    assert!(offset_of!(D3d10RasterizerDesc, antialiased_line_enable) == 36);
    assert!(size_of::<D3d10SamplerDesc>() == 52);
    assert!(offset_of!(D3d10SamplerDesc, filter) == 0);
    assert!(offset_of!(D3d10SamplerDesc, address_u) == 4);
    assert!(offset_of!(D3d10SamplerDesc, address_v) == 8);
    assert!(offset_of!(D3d10SamplerDesc, address_w) == 12);
    assert!(offset_of!(D3d10SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(D3d10SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(D3d10SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(D3d10SamplerDesc, border_color) == 28);
    assert!(offset_of!(D3d10SamplerDesc, min_lod) == 44);
    assert!(offset_of!(D3d10SamplerDesc, max_lod) == 48);
    assert!(size_of::<D3d10InputElementDesc>() == 32);
    assert!(offset_of!(D3d10InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(D3d10InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(D3d10InputElementDesc, format) == 12);
    assert!(offset_of!(D3d10InputElementDesc, input_slot) == 16);
    assert!(offset_of!(D3d10InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(D3d10InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(D3d10InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<D3d10SoDeclarationEntry>() == 16);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_name) == 0);
    assert!(offset_of!(D3d10SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(D3d10SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(D3d10SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(D3d10SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<D3d10QueryDesc>() == 8);
    assert!(offset_of!(D3d10QueryDesc, query) == 0);
    assert!(offset_of!(D3d10QueryDesc, misc_flags) == 4);
    assert!(size_of::<D3d10QueryDataTimestampDisjoint>() == 16);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, frequency) == 0);
    assert!(offset_of!(D3d10QueryDataTimestampDisjoint, disjoint) == 8);
    assert!(size_of::<D3d10QueryDataPipelineStatistics>() == 64);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_vertices) == 0);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ia_primitives) == 8);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, vs_invocations) == 16);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_invocations) == 24);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, gs_primitives) == 32);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_invocations) == 40);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, c_primitives) == 48);
    assert!(offset_of!(D3d10QueryDataPipelineStatistics, ps_invocations) == 56);
    assert!(size_of::<D3d10QueryDataSoStatistics>() == 16);
    assert!(offset_of!(D3d10QueryDataSoStatistics, num_primitives_written) == 0);
    assert!(offset_of!(D3d10QueryDataSoStatistics, primitives_storage_needed) == 8);
    assert!(size_of::<D3d10CounterDesc>() == 8);
    assert!(offset_of!(D3d10CounterDesc, counter) == 0);
    assert!(offset_of!(D3d10CounterDesc, misc_flags) == 4);
    assert!(size_of::<D3d10CounterInfo>() == 12);
    assert!(offset_of!(D3d10CounterInfo, last_device_dependent_counter) == 0);
    assert!(offset_of!(D3d10CounterInfo, num_simultaneous_counters) == 4);
    assert!(offset_of!(D3d10CounterInfo, num_detectable_parallel_units) == 8);
};
