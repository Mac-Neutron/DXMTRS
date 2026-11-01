//! C-layout mirrors for D3D12 resource views, clear values, ranges, and heaps (`d3d12.h`).

use core::mem::{ManuallyDrop, offset_of};

use super::resource::HeapProperties;

/// `D3D12_SRV_DIMENSION_UNKNOWN`.
pub const D3D12_SRV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_SRV_DIMENSION_BUFFER`.
pub const D3D12_SRV_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_SRV_DIMENSION_TEXTURE1D`.
pub const D3D12_SRV_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_SRV_DIMENSION_TEXTURE1DARRAY`.
pub const D3D12_SRV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
/// `D3D12_SRV_DIMENSION_TEXTURE2D`.
pub const D3D12_SRV_DIMENSION_TEXTURE2D: u32 = 4;
/// `D3D12_SRV_DIMENSION_TEXTURE2DARRAY`.
pub const D3D12_SRV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
/// `D3D12_SRV_DIMENSION_TEXTURE2DMS`.
pub const D3D12_SRV_DIMENSION_TEXTURE2DMS: u32 = 6;
/// `D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY`.
pub const D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
/// `D3D12_SRV_DIMENSION_TEXTURE3D`.
pub const D3D12_SRV_DIMENSION_TEXTURE3D: u32 = 8;
/// `D3D12_SRV_DIMENSION_TEXTURECUBE`.
pub const D3D12_SRV_DIMENSION_TEXTURECUBE: u32 = 9;
/// `D3D12_SRV_DIMENSION_TEXTURECUBEARRAY`.
pub const D3D12_SRV_DIMENSION_TEXTURECUBEARRAY: u32 = 10;
/// `D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE`.
pub const D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE: u32 = 11;

/// `D3D12_BUFFER_SRV_FLAG_NONE`.
pub const D3D12_BUFFER_SRV_FLAG_NONE: u32 = 0;
/// `D3D12_BUFFER_SRV_FLAG_RAW`.
pub const D3D12_BUFFER_SRV_FLAG_RAW: u32 = 0x1;

/// `D3D12_SHADER_COMPONENT_MAPPING_MASK`.
pub const D3D12_SHADER_COMPONENT_MAPPING_MASK: u32 = 0x7;
/// `D3D12_SHADER_COMPONENT_MAPPING_SHIFT`.
pub const D3D12_SHADER_COMPONENT_MAPPING_SHIFT: u32 = 3;
/// `D3D12_SHADER_COMPONENT_MAPPING_ALWAYS_SET_BIT_AVOIDING_ZEROMEM_MISTAKES`.
pub const D3D12_SHADER_COMPONENT_MAPPING_ALWAYS_SET_BIT_AVOIDING_ZEROMEM_MISTAKES: u32 =
    1 << (D3D12_SHADER_COMPONENT_MAPPING_SHIFT * 4);
/// `D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING` (`kDefaultShader4Component`, `d3d12_device.hpp:495`).
pub const D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING: u32 = 0b1_011_010_001_000;

/// `D3D12_UAV_DIMENSION_UNKNOWN`.
pub const D3D12_UAV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_UAV_DIMENSION_BUFFER`.
pub const D3D12_UAV_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_UAV_DIMENSION_TEXTURE1D`.
pub const D3D12_UAV_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_UAV_DIMENSION_TEXTURE1DARRAY`.
pub const D3D12_UAV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
/// `D3D12_UAV_DIMENSION_TEXTURE2D`.
pub const D3D12_UAV_DIMENSION_TEXTURE2D: u32 = 4;
/// `D3D12_UAV_DIMENSION_TEXTURE2DARRAY`.
pub const D3D12_UAV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
/// `D3D12_UAV_DIMENSION_TEXTURE2DMS`.
pub const D3D12_UAV_DIMENSION_TEXTURE2DMS: u32 = 6;
/// `D3D12_UAV_DIMENSION_TEXTURE2DMSARRAY`.
pub const D3D12_UAV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
/// `D3D12_UAV_DIMENSION_TEXTURE3D`.
pub const D3D12_UAV_DIMENSION_TEXTURE3D: u32 = 8;

/// `D3D12_BUFFER_UAV_FLAG_NONE`.
pub const D3D12_BUFFER_UAV_FLAG_NONE: u32 = 0;
/// `D3D12_BUFFER_UAV_FLAG_RAW`.
pub const D3D12_BUFFER_UAV_FLAG_RAW: u32 = 0x1;

/// `D3D12_RTV_DIMENSION_UNKNOWN`.
pub const D3D12_RTV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_RTV_DIMENSION_BUFFER`.
pub const D3D12_RTV_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_RTV_DIMENSION_TEXTURE1D`.
pub const D3D12_RTV_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_RTV_DIMENSION_TEXTURE1DARRAY`.
pub const D3D12_RTV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
/// `D3D12_RTV_DIMENSION_TEXTURE2D`.
pub const D3D12_RTV_DIMENSION_TEXTURE2D: u32 = 4;
/// `D3D12_RTV_DIMENSION_TEXTURE2DARRAY`.
pub const D3D12_RTV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
/// `D3D12_RTV_DIMENSION_TEXTURE2DMS`.
pub const D3D12_RTV_DIMENSION_TEXTURE2DMS: u32 = 6;
/// `D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY`.
pub const D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
/// `D3D12_RTV_DIMENSION_TEXTURE3D`.
pub const D3D12_RTV_DIMENSION_TEXTURE3D: u32 = 8;

/// `D3D12_DSV_DIMENSION_UNKNOWN`.
pub const D3D12_DSV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_DSV_DIMENSION_TEXTURE1D`.
pub const D3D12_DSV_DIMENSION_TEXTURE1D: u32 = 1;
/// `D3D12_DSV_DIMENSION_TEXTURE1DARRAY`.
pub const D3D12_DSV_DIMENSION_TEXTURE1DARRAY: u32 = 2;
/// `D3D12_DSV_DIMENSION_TEXTURE2D`.
pub const D3D12_DSV_DIMENSION_TEXTURE2D: u32 = 3;
/// `D3D12_DSV_DIMENSION_TEXTURE2DARRAY`.
pub const D3D12_DSV_DIMENSION_TEXTURE2DARRAY: u32 = 4;
/// `D3D12_DSV_DIMENSION_TEXTURE2DMS`.
pub const D3D12_DSV_DIMENSION_TEXTURE2DMS: u32 = 5;
/// `D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY`.
pub const D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY: u32 = 6;

/// `D3D12_DSV_FLAG_NONE`.
pub const D3D12_DSV_FLAG_NONE: u32 = 0;
/// `D3D12_DSV_FLAG_READ_ONLY_DEPTH`.
pub const D3D12_DSV_FLAG_READ_ONLY_DEPTH: u32 = 0x1;
/// `D3D12_DSV_FLAG_READ_ONLY_STENCIL`.
pub const D3D12_DSV_FLAG_READ_ONLY_STENCIL: u32 = 0x2;

/// `D3D12_HEAP_FLAG_NONE`.
pub const D3D12_HEAP_FLAG_NONE: u32 = 0;
/// `D3D12_HEAP_FLAG_DENY_BUFFERS`.
pub const D3D12_HEAP_FLAG_DENY_BUFFERS: u32 = 0x4;
/// `D3D12_HEAP_FLAG_DENY_RT_DS_TEXTURES`.
pub const D3D12_HEAP_FLAG_DENY_RT_DS_TEXTURES: u32 = 0x40;
/// `D3D12_HEAP_FLAG_DENY_NON_RT_DS_TEXTURES`.
pub const D3D12_HEAP_FLAG_DENY_NON_RT_DS_TEXTURES: u32 = 0x80;

/// `D3D12_CONSTANT_BUFFER_VIEW_DESC`.
#[repr(C)]
pub struct ConstantBufferViewDesc {
    pub buffer_location: u64,
    pub size_in_bytes: u32,
}

/// `D3D12_BUFFER_SRV`.
#[repr(C)]
pub struct BufferSrv {
    pub first_element: u64,
    pub num_elements: u32,
    pub structure_byte_stride: u32,
    pub flags: u32,
}

/// `D3D12_TEX1D_SRV`.
#[repr(C)]
pub struct Tex1dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEX1D_ARRAY_SRV`.
#[repr(C)]
pub struct Tex1dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEX2D_SRV`.
#[repr(C)]
pub struct Tex2dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub plane_slice: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEX2D_ARRAY_SRV`.
#[repr(C)]
pub struct Tex2dArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEX2DMS_SRV`.
#[repr(C)]
pub struct Tex2dmsSrv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D12_TEX2DMS_ARRAY_SRV`.
#[repr(C)]
pub struct Tex2dmsArraySrv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX3D_SRV`.
#[repr(C)]
pub struct Tex3dSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEXCUBE_SRV`.
#[repr(C)]
pub struct TexCubeSrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_TEXCUBE_ARRAY_SRV`.
#[repr(C)]
pub struct TexCubeArraySrv {
    pub most_detailed_mip: u32,
    pub mip_levels: u32,
    pub first_2d_array_face: u32,
    pub num_cubes: u32,
    pub resource_min_lod_clamp: f32,
}

/// `D3D12_RAYTRACING_ACCELERATION_STRUCTURE_SRV`.
#[repr(C)]
pub struct RaytracingAccelerationStructureSrv {
    pub location: u64,
}

/// Union body of [`ShaderResourceViewDesc`].
#[repr(C)]
pub union ShaderResourceViewDescBody {
    pub buffer: ManuallyDrop<BufferSrv>,
    pub texture_1d: ManuallyDrop<Tex1dSrv>,
    pub texture_1d_array: ManuallyDrop<Tex1dArraySrv>,
    pub texture_2d: ManuallyDrop<Tex2dSrv>,
    pub texture_2d_array: ManuallyDrop<Tex2dArraySrv>,
    pub texture_2dms: ManuallyDrop<Tex2dmsSrv>,
    pub texture_2dms_array: ManuallyDrop<Tex2dmsArraySrv>,
    pub texture_3d: ManuallyDrop<Tex3dSrv>,
    pub texture_cube: ManuallyDrop<TexCubeSrv>,
    pub texture_cube_array: ManuallyDrop<TexCubeArraySrv>,
    pub raytracing_acceleration_structure: ManuallyDrop<RaytracingAccelerationStructureSrv>,
}

/// `D3D12_SHADER_RESOURCE_VIEW_DESC`.
#[repr(C)]
pub struct ShaderResourceViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub shader_4_component_mapping: u32,
    pub view: ShaderResourceViewDescBody,
}

/// `D3D12_BUFFER_UAV`.
#[repr(C)]
pub struct BufferUav {
    pub first_element: u64,
    pub num_elements: u32,
    pub structure_byte_stride: u32,
    pub counter_offset_in_bytes: u64,
    pub flags: u32,
}

/// `D3D12_TEX1D_UAV`.
#[repr(C)]
pub struct Tex1dUav {
    pub mip_slice: u32,
}

/// `D3D12_TEX1D_ARRAY_UAV`.
#[repr(C)]
pub struct Tex1dArrayUav {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX2D_UAV`.
#[repr(C)]
pub struct Tex2dUav {
    pub mip_slice: u32,
    pub plane_slice: u32,
}

/// `D3D12_TEX2D_ARRAY_UAV`.
#[repr(C)]
pub struct Tex2dArrayUav {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
}

/// `D3D12_TEX2DMS_UAV`.
#[repr(C)]
pub struct Tex2dmsUav {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D12_TEX2DMS_ARRAY_UAV`.
#[repr(C)]
pub struct Tex2dmsArrayUav {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX3D_UAV`.
#[repr(C)]
pub struct Tex3dUav {
    pub mip_slice: u32,
    pub first_w_slice: u32,
    pub w_size: u32,
}

/// Union body of [`UnorderedAccessViewDesc`].
#[repr(C)]
pub union UnorderedAccessViewDescBody {
    pub buffer: ManuallyDrop<BufferUav>,
    pub texture_1d: ManuallyDrop<Tex1dUav>,
    pub texture_1d_array: ManuallyDrop<Tex1dArrayUav>,
    pub texture_2d: ManuallyDrop<Tex2dUav>,
    pub texture_2d_array: ManuallyDrop<Tex2dArrayUav>,
    pub texture_2dms: ManuallyDrop<Tex2dmsUav>,
    pub texture_2dms_array: ManuallyDrop<Tex2dmsArrayUav>,
    pub texture_3d: ManuallyDrop<Tex3dUav>,
}

/// `D3D12_UNORDERED_ACCESS_VIEW_DESC`.
#[repr(C)]
pub struct UnorderedAccessViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub view: UnorderedAccessViewDescBody,
}

/// `D3D12_BUFFER_RTV`.
#[repr(C)]
pub struct BufferRtv {
    pub first_element: u64,
    pub num_elements: u32,
}

/// `D3D12_TEX1D_RTV`.
#[repr(C)]
pub struct Tex1dRtv {
    pub mip_slice: u32,
}

/// `D3D12_TEX1D_ARRAY_RTV`.
#[repr(C)]
pub struct Tex1dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX2D_RTV`.
#[repr(C)]
pub struct Tex2dRtv {
    pub mip_slice: u32,
    pub plane_slice: u32,
}

/// `D3D12_TEX2D_ARRAY_RTV`.
#[repr(C)]
pub struct Tex2dArrayRtv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
    pub plane_slice: u32,
}

/// `D3D12_TEX2DMS_RTV`.
#[repr(C)]
pub struct Tex2dmsRtv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D12_TEX2DMS_ARRAY_RTV`.
#[repr(C)]
pub struct Tex2dmsArrayRtv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX3D_RTV`.
#[repr(C)]
pub struct Tex3dRtv {
    pub mip_slice: u32,
    pub first_w_slice: u32,
    pub w_size: u32,
}

/// Union body of [`RenderTargetViewDesc`].
#[repr(C)]
pub union RenderTargetViewDescBody {
    pub buffer: ManuallyDrop<BufferRtv>,
    pub texture_1d: ManuallyDrop<Tex1dRtv>,
    pub texture_1d_array: ManuallyDrop<Tex1dArrayRtv>,
    pub texture_2d: ManuallyDrop<Tex2dRtv>,
    pub texture_2d_array: ManuallyDrop<Tex2dArrayRtv>,
    pub texture_2dms: ManuallyDrop<Tex2dmsRtv>,
    pub texture_2dms_array: ManuallyDrop<Tex2dmsArrayRtv>,
    pub texture_3d: ManuallyDrop<Tex3dRtv>,
}

/// `D3D12_RENDER_TARGET_VIEW_DESC`.
#[repr(C)]
pub struct RenderTargetViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub view: RenderTargetViewDescBody,
}

/// `D3D12_TEX1D_DSV`.
#[repr(C)]
pub struct Tex1dDsv {
    pub mip_slice: u32,
}

/// `D3D12_TEX1D_ARRAY_DSV`.
#[repr(C)]
pub struct Tex1dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX2D_DSV`.
#[repr(C)]
pub struct Tex2dDsv {
    pub mip_slice: u32,
}

/// `D3D12_TEX2D_ARRAY_DSV`.
#[repr(C)]
pub struct Tex2dArrayDsv {
    pub mip_slice: u32,
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// `D3D12_TEX2DMS_DSV`.
#[repr(C)]
pub struct Tex2dmsDsv {
    pub unused_field_nothing_to_define: u32,
}

/// `D3D12_TEX2DMS_ARRAY_DSV`.
#[repr(C)]
pub struct Tex2dmsArrayDsv {
    pub first_array_slice: u32,
    pub array_size: u32,
}

/// Union body of [`DepthStencilViewDesc`].
#[repr(C)]
pub union DepthStencilViewDescBody {
    pub texture_1d: ManuallyDrop<Tex1dDsv>,
    pub texture_1d_array: ManuallyDrop<Tex1dArrayDsv>,
    pub texture_2d: ManuallyDrop<Tex2dDsv>,
    pub texture_2d_array: ManuallyDrop<Tex2dArrayDsv>,
    pub texture_2dms: ManuallyDrop<Tex2dmsDsv>,
    pub texture_2dms_array: ManuallyDrop<Tex2dmsArrayDsv>,
}

/// `D3D12_DEPTH_STENCIL_VIEW_DESC`.
#[repr(C)]
pub struct DepthStencilViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub flags: u32,
    pub view: DepthStencilViewDescBody,
}

/// `D3D12_DEPTH_STENCIL_VALUE`.
#[repr(C)]
pub struct DepthStencilValue {
    pub depth: f32,
    pub stencil: u8,
}

/// Union body of [`ClearValue`].
#[repr(C)]
pub union ClearValueBody {
    pub color: [f32; 4],
    pub depth_stencil: ManuallyDrop<DepthStencilValue>,
}

/// `D3D12_CLEAR_VALUE`.
#[repr(C)]
pub struct ClearValue {
    pub format: u32,
    pub value: ClearValueBody,
}

/// `D3D12_RANGE`.
#[repr(C)]
pub struct D3d12Range {
    pub begin: usize,
    pub end: usize,
}

/// `D3D12_HEAP_DESC`.
#[repr(C)]
pub struct D3d12HeapDesc {
    pub size_in_bytes: u64,
    pub properties: HeapProperties,
    pub alignment: u64,
    pub flags: u32,
}

// Clang `-fdump-record-layouts` checks across all five targets.
const _: () = {
    assert!(size_of::<ConstantBufferViewDesc>() == 16 && align_of::<ConstantBufferViewDesc>() == 8);
    assert!(offset_of!(ConstantBufferViewDesc, buffer_location) == 0);
    assert!(offset_of!(ConstantBufferViewDesc, size_in_bytes) == 8);

    assert!(size_of::<BufferSrv>() == 24 && align_of::<BufferSrv>() == 8);
    assert!(offset_of!(BufferSrv, first_element) == 0);
    assert!(offset_of!(BufferSrv, num_elements) == 8);
    assert!(offset_of!(BufferSrv, structure_byte_stride) == 12);
    assert!(offset_of!(BufferSrv, flags) == 16);

    assert!(size_of::<Tex1dSrv>() == 12 && align_of::<Tex1dSrv>() == 4);
    assert!(offset_of!(Tex1dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(Tex1dSrv, mip_levels) == 4);
    assert!(offset_of!(Tex1dSrv, resource_min_lod_clamp) == 8);

    assert!(size_of::<Tex1dArraySrv>() == 20 && align_of::<Tex1dArraySrv>() == 4);
    assert!(offset_of!(Tex1dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(Tex1dArraySrv, mip_levels) == 4);
    assert!(offset_of!(Tex1dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(Tex1dArraySrv, array_size) == 12);
    assert!(offset_of!(Tex1dArraySrv, resource_min_lod_clamp) == 16);

    assert!(size_of::<Tex2dSrv>() == 16 && align_of::<Tex2dSrv>() == 4);
    assert!(offset_of!(Tex2dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(Tex2dSrv, mip_levels) == 4);
    assert!(offset_of!(Tex2dSrv, plane_slice) == 8);
    assert!(offset_of!(Tex2dSrv, resource_min_lod_clamp) == 12);

    assert!(size_of::<Tex2dArraySrv>() == 24 && align_of::<Tex2dArraySrv>() == 4);
    assert!(offset_of!(Tex2dArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(Tex2dArraySrv, mip_levels) == 4);
    assert!(offset_of!(Tex2dArraySrv, first_array_slice) == 8);
    assert!(offset_of!(Tex2dArraySrv, array_size) == 12);
    assert!(offset_of!(Tex2dArraySrv, plane_slice) == 16);
    assert!(offset_of!(Tex2dArraySrv, resource_min_lod_clamp) == 20);

    assert!(size_of::<Tex2dmsSrv>() == 4 && align_of::<Tex2dmsSrv>() == 4);
    assert!(offset_of!(Tex2dmsSrv, unused_field_nothing_to_define) == 0);

    assert!(size_of::<Tex2dmsArraySrv>() == 8 && align_of::<Tex2dmsArraySrv>() == 4);
    assert!(offset_of!(Tex2dmsArraySrv, first_array_slice) == 0);
    assert!(offset_of!(Tex2dmsArraySrv, array_size) == 4);

    assert!(size_of::<Tex3dSrv>() == 12 && align_of::<Tex3dSrv>() == 4);
    assert!(offset_of!(Tex3dSrv, most_detailed_mip) == 0);
    assert!(offset_of!(Tex3dSrv, mip_levels) == 4);
    assert!(offset_of!(Tex3dSrv, resource_min_lod_clamp) == 8);

    assert!(size_of::<TexCubeSrv>() == 12 && align_of::<TexCubeSrv>() == 4);
    assert!(offset_of!(TexCubeSrv, most_detailed_mip) == 0);
    assert!(offset_of!(TexCubeSrv, mip_levels) == 4);
    assert!(offset_of!(TexCubeSrv, resource_min_lod_clamp) == 8);

    assert!(size_of::<TexCubeArraySrv>() == 20 && align_of::<TexCubeArraySrv>() == 4);
    assert!(offset_of!(TexCubeArraySrv, most_detailed_mip) == 0);
    assert!(offset_of!(TexCubeArraySrv, mip_levels) == 4);
    assert!(offset_of!(TexCubeArraySrv, first_2d_array_face) == 8);
    assert!(offset_of!(TexCubeArraySrv, num_cubes) == 12);
    assert!(offset_of!(TexCubeArraySrv, resource_min_lod_clamp) == 16);

    assert!(
        size_of::<RaytracingAccelerationStructureSrv>() == 8
            && align_of::<RaytracingAccelerationStructureSrv>() == 8
    );
    assert!(offset_of!(RaytracingAccelerationStructureSrv, location) == 0);

    assert!(size_of::<ShaderResourceViewDesc>() == 40 && align_of::<ShaderResourceViewDesc>() == 8);
    assert!(offset_of!(ShaderResourceViewDesc, format) == 0);
    assert!(offset_of!(ShaderResourceViewDesc, view_dimension) == 4);
    assert!(offset_of!(ShaderResourceViewDesc, shader_4_component_mapping) == 8);
    assert!(offset_of!(ShaderResourceViewDesc, view) == 16);

    assert!(size_of::<BufferUav>() == 32 && align_of::<BufferUav>() == 8);
    assert!(offset_of!(BufferUav, first_element) == 0);
    assert!(offset_of!(BufferUav, num_elements) == 8);
    assert!(offset_of!(BufferUav, structure_byte_stride) == 12);
    assert!(offset_of!(BufferUav, counter_offset_in_bytes) == 16);
    assert!(offset_of!(BufferUav, flags) == 24);

    assert!(size_of::<Tex1dUav>() == 4 && align_of::<Tex1dUav>() == 4);
    assert!(offset_of!(Tex1dUav, mip_slice) == 0);

    assert!(size_of::<Tex1dArrayUav>() == 12 && align_of::<Tex1dArrayUav>() == 4);
    assert!(offset_of!(Tex1dArrayUav, mip_slice) == 0);
    assert!(offset_of!(Tex1dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(Tex1dArrayUav, array_size) == 8);

    assert!(size_of::<Tex2dUav>() == 8 && align_of::<Tex2dUav>() == 4);
    assert!(offset_of!(Tex2dUav, mip_slice) == 0);
    assert!(offset_of!(Tex2dUav, plane_slice) == 4);

    assert!(size_of::<Tex2dArrayUav>() == 16 && align_of::<Tex2dArrayUav>() == 4);
    assert!(offset_of!(Tex2dArrayUav, mip_slice) == 0);
    assert!(offset_of!(Tex2dArrayUav, first_array_slice) == 4);
    assert!(offset_of!(Tex2dArrayUav, array_size) == 8);
    assert!(offset_of!(Tex2dArrayUav, plane_slice) == 12);

    assert!(size_of::<Tex2dmsUav>() == 4 && align_of::<Tex2dmsUav>() == 4);
    assert!(offset_of!(Tex2dmsUav, unused_field_nothing_to_define) == 0);

    assert!(size_of::<Tex2dmsArrayUav>() == 8 && align_of::<Tex2dmsArrayUav>() == 4);
    assert!(offset_of!(Tex2dmsArrayUav, first_array_slice) == 0);
    assert!(offset_of!(Tex2dmsArrayUav, array_size) == 4);

    assert!(size_of::<Tex3dUav>() == 12 && align_of::<Tex3dUav>() == 4);
    assert!(offset_of!(Tex3dUav, mip_slice) == 0);
    assert!(offset_of!(Tex3dUav, first_w_slice) == 4);
    assert!(offset_of!(Tex3dUav, w_size) == 8);

    assert!(
        size_of::<UnorderedAccessViewDesc>() == 40 && align_of::<UnorderedAccessViewDesc>() == 8
    );
    assert!(offset_of!(UnorderedAccessViewDesc, format) == 0);
    assert!(offset_of!(UnorderedAccessViewDesc, view_dimension) == 4);
    assert!(offset_of!(UnorderedAccessViewDesc, view) == 8);

    assert!(size_of::<BufferRtv>() == 16 && align_of::<BufferRtv>() == 8);
    assert!(offset_of!(BufferRtv, first_element) == 0);
    assert!(offset_of!(BufferRtv, num_elements) == 8);

    assert!(size_of::<Tex1dRtv>() == 4 && align_of::<Tex1dRtv>() == 4);
    assert!(offset_of!(Tex1dRtv, mip_slice) == 0);

    assert!(size_of::<Tex1dArrayRtv>() == 12 && align_of::<Tex1dArrayRtv>() == 4);
    assert!(offset_of!(Tex1dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(Tex1dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(Tex1dArrayRtv, array_size) == 8);

    assert!(size_of::<Tex2dRtv>() == 8 && align_of::<Tex2dRtv>() == 4);
    assert!(offset_of!(Tex2dRtv, mip_slice) == 0);
    assert!(offset_of!(Tex2dRtv, plane_slice) == 4);

    assert!(size_of::<Tex2dArrayRtv>() == 16 && align_of::<Tex2dArrayRtv>() == 4);
    assert!(offset_of!(Tex2dArrayRtv, mip_slice) == 0);
    assert!(offset_of!(Tex2dArrayRtv, first_array_slice) == 4);
    assert!(offset_of!(Tex2dArrayRtv, array_size) == 8);
    assert!(offset_of!(Tex2dArrayRtv, plane_slice) == 12);

    assert!(size_of::<Tex2dmsRtv>() == 4 && align_of::<Tex2dmsRtv>() == 4);
    assert!(offset_of!(Tex2dmsRtv, unused_field_nothing_to_define) == 0);

    assert!(size_of::<Tex2dmsArrayRtv>() == 8 && align_of::<Tex2dmsArrayRtv>() == 4);
    assert!(offset_of!(Tex2dmsArrayRtv, first_array_slice) == 0);
    assert!(offset_of!(Tex2dmsArrayRtv, array_size) == 4);

    assert!(size_of::<Tex3dRtv>() == 12 && align_of::<Tex3dRtv>() == 4);
    assert!(offset_of!(Tex3dRtv, mip_slice) == 0);
    assert!(offset_of!(Tex3dRtv, first_w_slice) == 4);
    assert!(offset_of!(Tex3dRtv, w_size) == 8);

    assert!(size_of::<RenderTargetViewDesc>() == 24 && align_of::<RenderTargetViewDesc>() == 8);
    assert!(offset_of!(RenderTargetViewDesc, format) == 0);
    assert!(offset_of!(RenderTargetViewDesc, view_dimension) == 4);
    assert!(offset_of!(RenderTargetViewDesc, view) == 8);

    assert!(size_of::<Tex1dDsv>() == 4 && align_of::<Tex1dDsv>() == 4);
    assert!(offset_of!(Tex1dDsv, mip_slice) == 0);

    assert!(size_of::<Tex1dArrayDsv>() == 12 && align_of::<Tex1dArrayDsv>() == 4);
    assert!(offset_of!(Tex1dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(Tex1dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(Tex1dArrayDsv, array_size) == 8);

    assert!(size_of::<Tex2dDsv>() == 4 && align_of::<Tex2dDsv>() == 4);
    assert!(offset_of!(Tex2dDsv, mip_slice) == 0);

    assert!(size_of::<Tex2dArrayDsv>() == 12 && align_of::<Tex2dArrayDsv>() == 4);
    assert!(offset_of!(Tex2dArrayDsv, mip_slice) == 0);
    assert!(offset_of!(Tex2dArrayDsv, first_array_slice) == 4);
    assert!(offset_of!(Tex2dArrayDsv, array_size) == 8);

    assert!(size_of::<Tex2dmsDsv>() == 4 && align_of::<Tex2dmsDsv>() == 4);
    assert!(offset_of!(Tex2dmsDsv, unused_field_nothing_to_define) == 0);

    assert!(size_of::<Tex2dmsArrayDsv>() == 8 && align_of::<Tex2dmsArrayDsv>() == 4);
    assert!(offset_of!(Tex2dmsArrayDsv, first_array_slice) == 0);
    assert!(offset_of!(Tex2dmsArrayDsv, array_size) == 4);

    assert!(size_of::<DepthStencilViewDesc>() == 24 && align_of::<DepthStencilViewDesc>() == 4);
    assert!(offset_of!(DepthStencilViewDesc, format) == 0);
    assert!(offset_of!(DepthStencilViewDesc, view_dimension) == 4);
    assert!(offset_of!(DepthStencilViewDesc, flags) == 8);
    assert!(offset_of!(DepthStencilViewDesc, view) == 12);

    assert!(size_of::<DepthStencilValue>() == 8 && align_of::<DepthStencilValue>() == 4);
    assert!(offset_of!(DepthStencilValue, depth) == 0);
    assert!(offset_of!(DepthStencilValue, stencil) == 4);

    assert!(size_of::<ClearValue>() == 20 && align_of::<ClearValue>() == 4);
    assert!(offset_of!(ClearValue, format) == 0);
    assert!(offset_of!(ClearValue, value) == 4);

    assert!(
        size_of::<D3d12Range>() == 2 * size_of::<usize>()
            && align_of::<D3d12Range>() == align_of::<usize>()
    );
    assert!(offset_of!(D3d12Range, begin) == 0);
    assert!(offset_of!(D3d12Range, end) == size_of::<usize>());

    assert!(size_of::<D3d12HeapDesc>() == 48 && align_of::<D3d12HeapDesc>() == 8);
    assert!(offset_of!(D3d12HeapDesc, size_in_bytes) == 0);
    assert!(offset_of!(D3d12HeapDesc, properties) == 8);
    assert!(offset_of!(D3d12HeapDesc, alignment) == 32);
    assert!(offset_of!(D3d12HeapDesc, flags) == 40);
};
