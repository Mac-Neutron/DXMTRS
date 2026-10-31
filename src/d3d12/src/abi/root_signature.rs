//! The root signature descriptions, versions 1.0 and 1.1.
//!
//! `d3d12.h`'s structures, fields in snake case, and the constants their users name. A struct's
//! layout is the C's on every target, asserted at the end against clang's record layouts
//! (llvm-mingw's sysroot for the four PE targets, `include/native` natively). The C's anonymous
//! unions are named here (`VersionedDesc`, `*Payload`); their arms are `ManuallyDrop`, which a
//! union field may hold without a `Copy` type. `D3D12_STATIC_SAMPLER_DESC` lives here, as the
//! descriptions hold it; its filter, address, comparison and border enumerations are the
//! samplers'.

use core::mem::{ManuallyDrop, offset_of};

pub const D3D_ROOT_SIGNATURE_VERSION_1_0: u32 = 1;
pub const D3D_ROOT_SIGNATURE_VERSION_1_1: u32 = 2;

pub const D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE: u32 = 0;
pub const D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS: u32 = 1;
pub const D3D12_ROOT_PARAMETER_TYPE_CBV: u32 = 2;
pub const D3D12_ROOT_PARAMETER_TYPE_SRV: u32 = 3;
pub const D3D12_ROOT_PARAMETER_TYPE_UAV: u32 = 4;

pub const D3D12_DESCRIPTOR_RANGE_TYPE_SRV: u32 = 0;
pub const D3D12_DESCRIPTOR_RANGE_TYPE_UAV: u32 = 1;
pub const D3D12_DESCRIPTOR_RANGE_TYPE_CBV: u32 = 2;
pub const D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER: u32 = 3;
pub const D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND: u32 = 0xffff_ffff;

pub const D3D12_DESCRIPTOR_RANGE_FLAG_NONE: u32 = 0;
pub const D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE: u32 = 0x1;
pub const D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE: u32 = 0x2;
pub const D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE: u32 = 0x4;
pub const D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC: u32 = 0x8;
pub const D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_STATIC_KEEPING_BUFFER_BOUNDS_CHECKS: u32 =
    0x10000;

pub const D3D12_ROOT_DESCRIPTOR_FLAG_NONE: u32 = 0;
pub const D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE: u32 = 0x2;
pub const D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE: u32 = 0x4;
pub const D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC: u32 = 0x8;

pub const D3D12_SHADER_VISIBILITY_ALL: u32 = 0;
pub const D3D12_SHADER_VISIBILITY_VERTEX: u32 = 1;
pub const D3D12_SHADER_VISIBILITY_HULL: u32 = 2;
pub const D3D12_SHADER_VISIBILITY_DOMAIN: u32 = 3;
pub const D3D12_SHADER_VISIBILITY_GEOMETRY: u32 = 4;
pub const D3D12_SHADER_VISIBILITY_PIXEL: u32 = 5;
pub const D3D12_SHADER_VISIBILITY_AMPLIFICATION: u32 = 6;
pub const D3D12_SHADER_VISIBILITY_MESH: u32 = 7;

pub const D3D12_ROOT_SIGNATURE_FLAG_NONE: u32 = 0;
pub const D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT: u32 = 0x1;
pub const D3D12_ROOT_SIGNATURE_FLAG_DENY_VERTEX_SHADER_ROOT_ACCESS: u32 = 0x2;
pub const D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS: u32 = 0x4;
pub const D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS: u32 = 0x8;
pub const D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS: u32 = 0x10;
pub const D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS: u32 = 0x20;
pub const D3D12_ROOT_SIGNATURE_FLAG_ALLOW_STREAM_OUTPUT: u32 = 0x40;
pub const D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED: u32 = 0x400;
pub const D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED: u32 = 0x800;

/// `D3D12_VERSIONED_ROOT_SIGNATURE_DESC`: `version` says which arm of `desc` holds it.
#[repr(C)]
pub struct VersionedRootSignatureDesc {
    pub version: u32,
    pub desc: VersionedDesc,
}

/// `D3D12_VERSIONED_ROOT_SIGNATURE_DESC`'s anonymous union.
#[repr(C)]
pub union VersionedDesc {
    pub desc_1_0: ManuallyDrop<RootSignatureDesc>,
    pub desc_1_1: ManuallyDrop<RootSignatureDesc1>,
}

/// `D3D12_ROOT_SIGNATURE_DESC`.
#[repr(C)]
pub struct RootSignatureDesc {
    pub num_parameters: u32,
    pub p_parameters: *const RootParameter,
    pub num_static_samplers: u32,
    pub p_static_samplers: *const StaticSamplerDesc,
    pub flags: u32,
}

/// `D3D12_ROOT_SIGNATURE_DESC1`.
#[repr(C)]
pub struct RootSignatureDesc1 {
    pub num_parameters: u32,
    pub p_parameters: *const RootParameter1,
    pub num_static_samplers: u32,
    pub p_static_samplers: *const StaticSamplerDesc,
    pub flags: u32,
}

/// `D3D12_ROOT_PARAMETER`: `parameter_type` says which arm of `payload` holds it.
#[repr(C)]
pub struct RootParameter {
    pub parameter_type: u32,
    pub payload: RootParameterPayload,
    pub shader_visibility: u32,
}

/// `D3D12_ROOT_PARAMETER`'s anonymous union.
#[repr(C)]
pub union RootParameterPayload {
    pub descriptor_table: ManuallyDrop<RootDescriptorTable>,
    pub constants: ManuallyDrop<RootConstants>,
    pub descriptor: ManuallyDrop<RootDescriptor>,
}

/// `D3D12_ROOT_PARAMETER1`: `parameter_type` says which arm of `payload` holds it.
#[repr(C)]
pub struct RootParameter1 {
    pub parameter_type: u32,
    pub payload: RootParameter1Payload,
    pub shader_visibility: u32,
}

/// `D3D12_ROOT_PARAMETER1`'s anonymous union.
#[repr(C)]
pub union RootParameter1Payload {
    pub descriptor_table: ManuallyDrop<RootDescriptorTable1>,
    pub constants: ManuallyDrop<RootConstants>,
    pub descriptor: ManuallyDrop<RootDescriptor1>,
}

/// `D3D12_ROOT_DESCRIPTOR_TABLE`.
#[repr(C)]
pub struct RootDescriptorTable {
    pub num_descriptor_ranges: u32,
    pub p_descriptor_ranges: *const DescriptorRange,
}

/// `D3D12_ROOT_DESCRIPTOR_TABLE1`.
#[repr(C)]
pub struct RootDescriptorTable1 {
    pub num_descriptor_ranges: u32,
    pub p_descriptor_ranges: *const DescriptorRange1,
}

/// `D3D12_DESCRIPTOR_RANGE`.
#[repr(C)]
pub struct DescriptorRange {
    pub range_type: u32,
    pub num_descriptors: u32,
    pub base_shader_register: u32,
    pub register_space: u32,
    pub offset_in_descriptors_from_table_start: u32,
}

/// `D3D12_DESCRIPTOR_RANGE1`.
#[repr(C)]
pub struct DescriptorRange1 {
    pub range_type: u32,
    pub num_descriptors: u32,
    pub base_shader_register: u32,
    pub register_space: u32,
    pub flags: u32,
    pub offset_in_descriptors_from_table_start: u32,
}

/// `D3D12_ROOT_CONSTANTS`.
#[repr(C)]
pub struct RootConstants {
    pub shader_register: u32,
    pub register_space: u32,
    pub num_32bit_values: u32,
}

/// `D3D12_ROOT_DESCRIPTOR`.
#[repr(C)]
pub struct RootDescriptor {
    pub shader_register: u32,
    pub register_space: u32,
}

/// `D3D12_ROOT_DESCRIPTOR1`.
#[repr(C)]
pub struct RootDescriptor1 {
    pub shader_register: u32,
    pub register_space: u32,
    pub flags: u32,
}

/// `D3D12_STATIC_SAMPLER_DESC`.
#[repr(C)]
pub struct StaticSamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: u32,
    pub min_lod: f32,
    pub max_lod: f32,
    pub shader_register: u32,
    pub register_space: u32,
    pub shader_visibility: u32,
}

// The layouts of clang's `-fdump-record-layouts` over `d3d12.h`: every mirrored struct's and
// union's size, alignment and field offsets, per target. The three 64-bit PE targets' dumps are
// the same.
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(size_of::<DescriptorRange>() == 20 && align_of::<DescriptorRange>() == 4);
    assert!(offset_of!(DescriptorRange, range_type) == 0);
    assert!(offset_of!(DescriptorRange, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange, register_space) == 12);
    assert!(offset_of!(DescriptorRange, offset_in_descriptors_from_table_start) == 16);
    assert!(size_of::<DescriptorRange1>() == 24 && align_of::<DescriptorRange1>() == 4);
    assert!(offset_of!(DescriptorRange1, range_type) == 0);
    assert!(offset_of!(DescriptorRange1, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange1, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange1, register_space) == 12);
    assert!(offset_of!(DescriptorRange1, flags) == 16);
    assert!(offset_of!(DescriptorRange1, offset_in_descriptors_from_table_start) == 20);
    assert!(size_of::<RootConstants>() == 12 && align_of::<RootConstants>() == 4);
    assert!(offset_of!(RootConstants, shader_register) == 0);
    assert!(offset_of!(RootConstants, register_space) == 4);
    assert!(offset_of!(RootConstants, num_32bit_values) == 8);
    assert!(size_of::<RootDescriptor>() == 8 && align_of::<RootDescriptor>() == 4);
    assert!(offset_of!(RootDescriptor, shader_register) == 0);
    assert!(offset_of!(RootDescriptor, register_space) == 4);
    assert!(size_of::<RootDescriptor1>() == 12 && align_of::<RootDescriptor1>() == 4);
    assert!(offset_of!(RootDescriptor1, shader_register) == 0);
    assert!(offset_of!(RootDescriptor1, register_space) == 4);
    assert!(offset_of!(RootDescriptor1, flags) == 8);
    assert!(size_of::<RootDescriptorTable>() == 8 && align_of::<RootDescriptorTable>() == 4);
    assert!(offset_of!(RootDescriptorTable, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable, p_descriptor_ranges) == 4);
    assert!(size_of::<RootDescriptorTable1>() == 8 && align_of::<RootDescriptorTable1>() == 4);
    assert!(offset_of!(RootDescriptorTable1, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable1, p_descriptor_ranges) == 4);
    assert!(size_of::<RootParameter>() == 20 && align_of::<RootParameter>() == 4);
    assert!(offset_of!(RootParameter, parameter_type) == 0);
    assert!(offset_of!(RootParameter, payload) == 4);
    assert!(offset_of!(RootParameter, shader_visibility) == 16);
    assert!(size_of::<RootParameter1>() == 20 && align_of::<RootParameter1>() == 4);
    assert!(offset_of!(RootParameter1, parameter_type) == 0);
    assert!(offset_of!(RootParameter1, payload) == 4);
    assert!(offset_of!(RootParameter1, shader_visibility) == 16);
    assert!(size_of::<RootParameter1Payload>() == 12 && align_of::<RootParameter1Payload>() == 4);
    assert!(offset_of!(RootParameter1Payload, descriptor_table) == 0);
    assert!(offset_of!(RootParameter1Payload, constants) == 0);
    assert!(offset_of!(RootParameter1Payload, descriptor) == 0);
    assert!(size_of::<RootParameterPayload>() == 12 && align_of::<RootParameterPayload>() == 4);
    assert!(offset_of!(RootParameterPayload, descriptor_table) == 0);
    assert!(offset_of!(RootParameterPayload, constants) == 0);
    assert!(offset_of!(RootParameterPayload, descriptor) == 0);
    assert!(size_of::<RootSignatureDesc>() == 20 && align_of::<RootSignatureDesc>() == 4);
    assert!(offset_of!(RootSignatureDesc, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc, p_parameters) == 4);
    assert!(offset_of!(RootSignatureDesc, num_static_samplers) == 8);
    assert!(offset_of!(RootSignatureDesc, p_static_samplers) == 12);
    assert!(offset_of!(RootSignatureDesc, flags) == 16);
    assert!(size_of::<RootSignatureDesc1>() == 20 && align_of::<RootSignatureDesc1>() == 4);
    assert!(offset_of!(RootSignatureDesc1, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc1, p_parameters) == 4);
    assert!(offset_of!(RootSignatureDesc1, num_static_samplers) == 8);
    assert!(offset_of!(RootSignatureDesc1, p_static_samplers) == 12);
    assert!(offset_of!(RootSignatureDesc1, flags) == 16);
    assert!(size_of::<StaticSamplerDesc>() == 52 && align_of::<StaticSamplerDesc>() == 4);
    assert!(offset_of!(StaticSamplerDesc, filter) == 0);
    assert!(offset_of!(StaticSamplerDesc, address_u) == 4);
    assert!(offset_of!(StaticSamplerDesc, address_v) == 8);
    assert!(offset_of!(StaticSamplerDesc, address_w) == 12);
    assert!(offset_of!(StaticSamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(StaticSamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(StaticSamplerDesc, comparison_func) == 24);
    assert!(offset_of!(StaticSamplerDesc, border_color) == 28);
    assert!(offset_of!(StaticSamplerDesc, min_lod) == 32);
    assert!(offset_of!(StaticSamplerDesc, max_lod) == 36);
    assert!(offset_of!(StaticSamplerDesc, shader_register) == 40);
    assert!(offset_of!(StaticSamplerDesc, register_space) == 44);
    assert!(offset_of!(StaticSamplerDesc, shader_visibility) == 48);
    assert!(size_of::<VersionedDesc>() == 20 && align_of::<VersionedDesc>() == 4);
    assert!(offset_of!(VersionedDesc, desc_1_0) == 0);
    assert!(offset_of!(VersionedDesc, desc_1_1) == 0);
    assert!(
        size_of::<VersionedRootSignatureDesc>() == 24
            && align_of::<VersionedRootSignatureDesc>() == 4
    );
    assert!(offset_of!(VersionedRootSignatureDesc, version) == 0);
    assert!(offset_of!(VersionedRootSignatureDesc, desc) == 4);
};
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
const _: () = {
    assert!(size_of::<DescriptorRange>() == 20 && align_of::<DescriptorRange>() == 4);
    assert!(offset_of!(DescriptorRange, range_type) == 0);
    assert!(offset_of!(DescriptorRange, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange, register_space) == 12);
    assert!(offset_of!(DescriptorRange, offset_in_descriptors_from_table_start) == 16);
    assert!(size_of::<DescriptorRange1>() == 24 && align_of::<DescriptorRange1>() == 4);
    assert!(offset_of!(DescriptorRange1, range_type) == 0);
    assert!(offset_of!(DescriptorRange1, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange1, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange1, register_space) == 12);
    assert!(offset_of!(DescriptorRange1, flags) == 16);
    assert!(offset_of!(DescriptorRange1, offset_in_descriptors_from_table_start) == 20);
    assert!(size_of::<RootConstants>() == 12 && align_of::<RootConstants>() == 4);
    assert!(offset_of!(RootConstants, shader_register) == 0);
    assert!(offset_of!(RootConstants, register_space) == 4);
    assert!(offset_of!(RootConstants, num_32bit_values) == 8);
    assert!(size_of::<RootDescriptor>() == 8 && align_of::<RootDescriptor>() == 4);
    assert!(offset_of!(RootDescriptor, shader_register) == 0);
    assert!(offset_of!(RootDescriptor, register_space) == 4);
    assert!(size_of::<RootDescriptor1>() == 12 && align_of::<RootDescriptor1>() == 4);
    assert!(offset_of!(RootDescriptor1, shader_register) == 0);
    assert!(offset_of!(RootDescriptor1, register_space) == 4);
    assert!(offset_of!(RootDescriptor1, flags) == 8);
    assert!(size_of::<RootDescriptorTable>() == 16 && align_of::<RootDescriptorTable>() == 8);
    assert!(offset_of!(RootDescriptorTable, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable, p_descriptor_ranges) == 8);
    assert!(size_of::<RootDescriptorTable1>() == 16 && align_of::<RootDescriptorTable1>() == 8);
    assert!(offset_of!(RootDescriptorTable1, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable1, p_descriptor_ranges) == 8);
    assert!(size_of::<RootParameter>() == 32 && align_of::<RootParameter>() == 8);
    assert!(offset_of!(RootParameter, parameter_type) == 0);
    assert!(offset_of!(RootParameter, payload) == 8);
    assert!(offset_of!(RootParameter, shader_visibility) == 24);
    assert!(size_of::<RootParameter1>() == 32 && align_of::<RootParameter1>() == 8);
    assert!(offset_of!(RootParameter1, parameter_type) == 0);
    assert!(offset_of!(RootParameter1, payload) == 8);
    assert!(offset_of!(RootParameter1, shader_visibility) == 24);
    assert!(size_of::<RootParameter1Payload>() == 16 && align_of::<RootParameter1Payload>() == 8);
    assert!(offset_of!(RootParameter1Payload, descriptor_table) == 0);
    assert!(offset_of!(RootParameter1Payload, constants) == 0);
    assert!(offset_of!(RootParameter1Payload, descriptor) == 0);
    assert!(size_of::<RootParameterPayload>() == 16 && align_of::<RootParameterPayload>() == 8);
    assert!(offset_of!(RootParameterPayload, descriptor_table) == 0);
    assert!(offset_of!(RootParameterPayload, constants) == 0);
    assert!(offset_of!(RootParameterPayload, descriptor) == 0);
    assert!(size_of::<RootSignatureDesc>() == 40 && align_of::<RootSignatureDesc>() == 8);
    assert!(offset_of!(RootSignatureDesc, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc, p_parameters) == 8);
    assert!(offset_of!(RootSignatureDesc, num_static_samplers) == 16);
    assert!(offset_of!(RootSignatureDesc, p_static_samplers) == 24);
    assert!(offset_of!(RootSignatureDesc, flags) == 32);
    assert!(size_of::<RootSignatureDesc1>() == 40 && align_of::<RootSignatureDesc1>() == 8);
    assert!(offset_of!(RootSignatureDesc1, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc1, p_parameters) == 8);
    assert!(offset_of!(RootSignatureDesc1, num_static_samplers) == 16);
    assert!(offset_of!(RootSignatureDesc1, p_static_samplers) == 24);
    assert!(offset_of!(RootSignatureDesc1, flags) == 32);
    assert!(size_of::<StaticSamplerDesc>() == 52 && align_of::<StaticSamplerDesc>() == 4);
    assert!(offset_of!(StaticSamplerDesc, filter) == 0);
    assert!(offset_of!(StaticSamplerDesc, address_u) == 4);
    assert!(offset_of!(StaticSamplerDesc, address_v) == 8);
    assert!(offset_of!(StaticSamplerDesc, address_w) == 12);
    assert!(offset_of!(StaticSamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(StaticSamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(StaticSamplerDesc, comparison_func) == 24);
    assert!(offset_of!(StaticSamplerDesc, border_color) == 28);
    assert!(offset_of!(StaticSamplerDesc, min_lod) == 32);
    assert!(offset_of!(StaticSamplerDesc, max_lod) == 36);
    assert!(offset_of!(StaticSamplerDesc, shader_register) == 40);
    assert!(offset_of!(StaticSamplerDesc, register_space) == 44);
    assert!(offset_of!(StaticSamplerDesc, shader_visibility) == 48);
    assert!(size_of::<VersionedDesc>() == 40 && align_of::<VersionedDesc>() == 8);
    assert!(offset_of!(VersionedDesc, desc_1_0) == 0);
    assert!(offset_of!(VersionedDesc, desc_1_1) == 0);
    assert!(
        size_of::<VersionedRootSignatureDesc>() == 48
            && align_of::<VersionedRootSignatureDesc>() == 8
    );
    assert!(offset_of!(VersionedRootSignatureDesc, version) == 0);
    assert!(offset_of!(VersionedRootSignatureDesc, desc) == 8);
};
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(size_of::<DescriptorRange>() == 20 && align_of::<DescriptorRange>() == 4);
    assert!(offset_of!(DescriptorRange, range_type) == 0);
    assert!(offset_of!(DescriptorRange, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange, register_space) == 12);
    assert!(offset_of!(DescriptorRange, offset_in_descriptors_from_table_start) == 16);
    assert!(size_of::<DescriptorRange1>() == 24 && align_of::<DescriptorRange1>() == 4);
    assert!(offset_of!(DescriptorRange1, range_type) == 0);
    assert!(offset_of!(DescriptorRange1, num_descriptors) == 4);
    assert!(offset_of!(DescriptorRange1, base_shader_register) == 8);
    assert!(offset_of!(DescriptorRange1, register_space) == 12);
    assert!(offset_of!(DescriptorRange1, flags) == 16);
    assert!(offset_of!(DescriptorRange1, offset_in_descriptors_from_table_start) == 20);
    assert!(size_of::<RootConstants>() == 12 && align_of::<RootConstants>() == 4);
    assert!(offset_of!(RootConstants, shader_register) == 0);
    assert!(offset_of!(RootConstants, register_space) == 4);
    assert!(offset_of!(RootConstants, num_32bit_values) == 8);
    assert!(size_of::<RootDescriptor>() == 8 && align_of::<RootDescriptor>() == 4);
    assert!(offset_of!(RootDescriptor, shader_register) == 0);
    assert!(offset_of!(RootDescriptor, register_space) == 4);
    assert!(size_of::<RootDescriptor1>() == 12 && align_of::<RootDescriptor1>() == 4);
    assert!(offset_of!(RootDescriptor1, shader_register) == 0);
    assert!(offset_of!(RootDescriptor1, register_space) == 4);
    assert!(offset_of!(RootDescriptor1, flags) == 8);
    assert!(size_of::<RootDescriptorTable>() == 16 && align_of::<RootDescriptorTable>() == 8);
    assert!(offset_of!(RootDescriptorTable, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable, p_descriptor_ranges) == 8);
    assert!(size_of::<RootDescriptorTable1>() == 16 && align_of::<RootDescriptorTable1>() == 8);
    assert!(offset_of!(RootDescriptorTable1, num_descriptor_ranges) == 0);
    assert!(offset_of!(RootDescriptorTable1, p_descriptor_ranges) == 8);
    assert!(size_of::<RootParameter>() == 32 && align_of::<RootParameter>() == 8);
    assert!(offset_of!(RootParameter, parameter_type) == 0);
    assert!(offset_of!(RootParameter, payload) == 8);
    assert!(offset_of!(RootParameter, shader_visibility) == 24);
    assert!(size_of::<RootParameter1>() == 32 && align_of::<RootParameter1>() == 8);
    assert!(offset_of!(RootParameter1, parameter_type) == 0);
    assert!(offset_of!(RootParameter1, payload) == 8);
    assert!(offset_of!(RootParameter1, shader_visibility) == 24);
    assert!(size_of::<RootParameter1Payload>() == 16 && align_of::<RootParameter1Payload>() == 8);
    assert!(offset_of!(RootParameter1Payload, descriptor_table) == 0);
    assert!(offset_of!(RootParameter1Payload, constants) == 0);
    assert!(offset_of!(RootParameter1Payload, descriptor) == 0);
    assert!(size_of::<RootParameterPayload>() == 16 && align_of::<RootParameterPayload>() == 8);
    assert!(offset_of!(RootParameterPayload, descriptor_table) == 0);
    assert!(offset_of!(RootParameterPayload, constants) == 0);
    assert!(offset_of!(RootParameterPayload, descriptor) == 0);
    assert!(size_of::<RootSignatureDesc>() == 40 && align_of::<RootSignatureDesc>() == 8);
    assert!(offset_of!(RootSignatureDesc, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc, p_parameters) == 8);
    assert!(offset_of!(RootSignatureDesc, num_static_samplers) == 16);
    assert!(offset_of!(RootSignatureDesc, p_static_samplers) == 24);
    assert!(offset_of!(RootSignatureDesc, flags) == 32);
    assert!(size_of::<RootSignatureDesc1>() == 40 && align_of::<RootSignatureDesc1>() == 8);
    assert!(offset_of!(RootSignatureDesc1, num_parameters) == 0);
    assert!(offset_of!(RootSignatureDesc1, p_parameters) == 8);
    assert!(offset_of!(RootSignatureDesc1, num_static_samplers) == 16);
    assert!(offset_of!(RootSignatureDesc1, p_static_samplers) == 24);
    assert!(offset_of!(RootSignatureDesc1, flags) == 32);
    assert!(size_of::<StaticSamplerDesc>() == 52 && align_of::<StaticSamplerDesc>() == 4);
    assert!(offset_of!(StaticSamplerDesc, filter) == 0);
    assert!(offset_of!(StaticSamplerDesc, address_u) == 4);
    assert!(offset_of!(StaticSamplerDesc, address_v) == 8);
    assert!(offset_of!(StaticSamplerDesc, address_w) == 12);
    assert!(offset_of!(StaticSamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(StaticSamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(StaticSamplerDesc, comparison_func) == 24);
    assert!(offset_of!(StaticSamplerDesc, border_color) == 28);
    assert!(offset_of!(StaticSamplerDesc, min_lod) == 32);
    assert!(offset_of!(StaticSamplerDesc, max_lod) == 36);
    assert!(offset_of!(StaticSamplerDesc, shader_register) == 40);
    assert!(offset_of!(StaticSamplerDesc, register_space) == 44);
    assert!(offset_of!(StaticSamplerDesc, shader_visibility) == 48);
    assert!(size_of::<VersionedDesc>() == 40 && align_of::<VersionedDesc>() == 8);
    assert!(offset_of!(VersionedDesc, desc_1_0) == 0);
    assert!(offset_of!(VersionedDesc, desc_1_1) == 0);
    assert!(
        size_of::<VersionedRootSignatureDesc>() == 48
            && align_of::<VersionedRootSignatureDesc>() == 8
    );
    assert!(offset_of!(VersionedRootSignatureDesc, version) == 0);
    assert!(offset_of!(VersionedRootSignatureDesc, desc) == 8);
};
