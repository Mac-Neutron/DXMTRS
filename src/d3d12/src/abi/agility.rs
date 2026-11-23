//! D3D12 Agility SDK ABI extensions beyond `d3d12.h`.
//!
//! `abi::vtbl` is generated up to `ID3D12Device10` and `ID3D12GraphicsCommandList7` against the
//! mingw-w64 `d3d12.h` header. This module embeds those base vtables at offset 0 and appends the
//! Agility SDK slots and parameter structures matching Microsoft `d3d12.idl` (`DirectX-Headers`).

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, align_of, offset_of, size_of},
};

use dxmt_util::com::{Guid, Hresult};

pub use super::root_signature::{
    D3D12_SAMPLER_FLAG_NON_NORMALIZED_COORDINATES, D3D12_SAMPLER_FLAG_NONE,
    D3D12_SAMPLER_FLAG_UINT_BORDER_COLOR,
};
use super::{
    resource::{ResourceAllocationInfo1, ResourceDesc1},
    vtbl::{
        D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle, ID3D12Device10Vtbl,
        ID3D12GraphicsCommandList7Vtbl,
    },
};

/// `D3D12_PROGRAM_TYPE_GENERIC_PIPELINE` (`d3d12.idl`).
pub const D3D12_PROGRAM_TYPE_GENERIC_PIPELINE: u32 = 1;
/// `D3D12_PROGRAM_TYPE_RAYTRACING_PIPELINE` (`d3d12.idl`).
pub const D3D12_PROGRAM_TYPE_RAYTRACING_PIPELINE: u32 = 4;
/// `D3D12_PROGRAM_TYPE_WORK_GRAPH` (`d3d12.idl`).
pub const D3D12_PROGRAM_TYPE_WORK_GRAPH: u32 = 5;
/// `D3D12_PROGRAM_TYPE_MLIR_PROGRAM` (`d3d12.idl`).
pub const D3D12_PROGRAM_TYPE_MLIR_PROGRAM: u32 = 6;

/// `D3D12_DISPATCH_MODE_NODE_CPU_INPUT` (`d3d12.idl`).
pub const D3D12_DISPATCH_MODE_NODE_CPU_INPUT: u32 = 0;
/// `D3D12_DISPATCH_MODE_NODE_GPU_INPUT` (`d3d12.idl`).
pub const D3D12_DISPATCH_MODE_NODE_GPU_INPUT: u32 = 1;
/// `D3D12_DISPATCH_MODE_MULTI_NODE_CPU_INPUT` (`d3d12.idl`).
pub const D3D12_DISPATCH_MODE_MULTI_NODE_CPU_INPUT: u32 = 2;
/// `D3D12_DISPATCH_MODE_MULTI_NODE_GPU_INPUT` (`d3d12.idl`).
pub const D3D12_DISPATCH_MODE_MULTI_NODE_GPU_INPUT: u32 = 3;
/// `D3D12_DISPATCH_MODE_MLIR_PROGRAM` (`d3d12.idl`).
pub const D3D12_DISPATCH_MODE_MLIR_PROGRAM: u32 = 4;

/// Anonymous border-color union in `D3D12_SAMPLER_DESC2` (`d3d12.idl`, size 16, align 4).
#[repr(C)]
pub union SamplerBorderColor {
    pub float_border_color: [f32; 4],
    pub uint_border_color: [u32; 4],
}

/// `D3D12_SAMPLER_DESC2` (`d3d12.idl`, size 56, align 4).
#[repr(C)]
pub struct SamplerDesc2 {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: SamplerBorderColor,
    pub min_lod: f32,
    pub max_lod: f32,
    pub flags: u32,
}

/// `D3D12_PROGRAM_IDENTIFIER` (`d3d12.idl`, size 32, align 8).
#[repr(C)]
pub struct ProgramIdentifier {
    pub opaque_data: [u64; 4],
}

/// `D3D12_GPU_VIRTUAL_ADDRESS_RANGE` (`d3d12.idl`, size 16, align 8).
#[repr(C)]
pub struct GpuVirtualAddressRange {
    pub start_address: u64,
    pub size_in_bytes: u64,
}

/// `D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE` (`d3d12.idl`, size 24, align 8).
#[repr(C)]
pub struct GpuVirtualAddressRangeAndStride {
    pub start_address: u64,
    pub size_in_bytes: u64,
    pub stride_in_bytes: u64,
}

/// `D3D12_SET_GENERIC_PIPELINE_DESC` (`d3d12.idl`, size 32, align 8).
#[repr(C)]
pub struct SetGenericPipelineDesc {
    pub program_identifier: ProgramIdentifier,
}

/// `D3D12_SET_RAYTRACING_PIPELINE_DESC` (`d3d12.idl`, size 32, align 8).
#[repr(C)]
pub struct SetRaytracingPipelineDesc {
    pub program_identifier: ProgramIdentifier,
}

/// `D3D12_SET_WORK_GRAPH_DESC` (`d3d12.idl`, size 80, align 8).
#[repr(C)]
pub struct SetWorkGraphDesc {
    pub program_identifier: ProgramIdentifier,
    pub flags: u32,
    pub backing_memory: GpuVirtualAddressRange,
    pub node_local_root_arguments_table: GpuVirtualAddressRangeAndStride,
}

/// `D3D12_SET_MLIR_PROGRAM_DESC` (`d3d12.idl`, size 40, align 8).
#[repr(C)]
pub struct SetMlirProgramDesc {
    pub program_identifier: ProgramIdentifier,
    pub flags: u32,
}

/// Anonymous union in `D3D12_SET_PROGRAM_DESC` (`d3d12.idl`, size 80, align 8).
#[repr(C)]
pub union SetProgramDescUnion {
    pub generic_pipeline: ManuallyDrop<SetGenericPipelineDesc>,
    pub raytracing_pipeline: ManuallyDrop<SetRaytracingPipelineDesc>,
    pub work_graph: ManuallyDrop<SetWorkGraphDesc>,
    pub mlir_program: ManuallyDrop<SetMlirProgramDesc>,
}

/// `D3D12_SET_PROGRAM_DESC` (`d3d12.idl`, size 88, align 8).
#[repr(C)]
pub struct SetProgramDesc {
    pub r#type: u32,
    pub u: SetProgramDescUnion,
}

/// `D3D12_NODE_CPU_INPUT` (`d3d12.idl`, size 24, align 8).
#[repr(C)]
pub struct NodeCpuInput {
    pub entrypoint_index: u32,
    pub num_records: u32,
    pub p_records: *const c_void,
    pub record_stride_in_bytes: u64,
}

/// `D3D12_MULTI_NODE_CPU_INPUT` (`d3d12.idl`, size 24 on 64-bit / 16 on 32-bit, align 8).
#[repr(C)]
pub struct MultiNodeCpuInput {
    pub num_node_inputs: u32,
    pub p_node_inputs: *const NodeCpuInput,
    pub node_input_stride_in_bytes: u64,
}

/// `D3D12_MLIR_PROGRAM_BINDINGS` (`d3d12.idl`, size 16 on 64-bit / 8 on 32-bit).
#[repr(C)]
pub struct MlirProgramBindings {
    pub num_bindings: u32,
    pub p_bindings: *const c_void,
}

/// Anonymous union in `D3D12_DISPATCH_GRAPH_DESC` (`d3d12.idl`, size 24, align 8).
#[repr(C)]
pub union DispatchGraphDescUnion {
    pub node_cpu_input: ManuallyDrop<NodeCpuInput>,
    pub node_gpu_input: u64,
    pub multi_node_cpu_input: ManuallyDrop<MultiNodeCpuInput>,
    pub multi_node_gpu_input: u64,
    pub mlir_program_bindings: ManuallyDrop<MlirProgramBindings>,
    pub gpu_descriptor_binding_align_check: D3d12GpuDescriptorHandle,
}

/// `D3D12_DISPATCH_GRAPH_DESC` (`d3d12.idl`, size 32, align 8).
#[repr(C)]
pub struct DispatchGraphDesc {
    pub mode: u32,
    pub u: DispatchGraphDescUnion,
}

/// `ID3D12Device14Vtbl`: `ID3D12Device10` (79 slots) plus `Device11`..`14` (4 slots, 83 total).
#[repr(C)]
pub struct ID3D12Device14Vtbl {
    pub base: ID3D12Device10Vtbl,
    /// `ID3D12Device11::CreateSampler2` (slot 79).
    pub create_sampler2: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        p_desc: *const SamplerDesc2,
        dest_descriptor: D3d12CpuDescriptorHandle,
    ),
    /// `ID3D12Device12::GetResourceAllocationInfo3` (slot 80, sret convention).
    pub get_resource_allocation_info3: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        ret: *mut c_void,
        visible_mask: u32,
        num_resource_descs: u32,
        p_resource_descs: *const ResourceDesc1,
        p_num_castable_formats: *const u32,
        pp_castable_formats: *const *const u32,
        p_resource_allocation_info1: *mut ResourceAllocationInfo1,
    ) -> *mut c_void,
    /// `ID3D12Device13::OpenExistingHeapFromAddress1` (slot 81).
    pub open_existing_heap_from_address1: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        p_address: *const c_void,
        size: usize,
        riid: *const Guid,
        ppv_heap: *mut *mut c_void,
    ) -> Hresult,
    /// `ID3D12Device14::CreateRootSignatureFromSubobjectInLibrary` (slot 82).
    pub create_root_signature_from_subobject_in_library:
        unsafe extern "system-unwind" fn(
            this: *mut c_void,
            node_mask: u32,
            p_library_blob: *const c_void,
            blob_length_in_bytes: usize,
            subobject_name: *const u16,
            riid: *const Guid,
            ppv_root_signature: *mut *mut c_void,
        ) -> Hresult,
}

/// `ID3D12GraphicsCommandList10Vtbl`: `List7` (81 slots) plus `List8`..`10` (5 slots, 86 total).
#[repr(C)]
pub struct ID3D12GraphicsCommandList10Vtbl {
    pub base: ID3D12GraphicsCommandList7Vtbl,
    /// `ID3D12GraphicsCommandList8::OMSetFrontAndBackStencilRef` (slot 81).
    pub om_set_front_and_back_stencil_ref: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        front_stencil_ref: u32,
        back_stencil_ref: u32,
    ),
    /// `ID3D12GraphicsCommandList9::RSSetDepthBias` (slot 82).
    pub rs_set_depth_bias: unsafe extern "system-unwind" fn(
        this: *mut c_void,
        depth_bias: f32,
        depth_bias_clamp: f32,
        slope_scaled_depth_bias: f32,
    ),
    /// `ID3D12GraphicsCommandList9::IASetIndexBufferStripCutValue` (slot 83).
    pub ia_set_index_buffer_strip_cut_value:
        unsafe extern "system-unwind" fn(this: *mut c_void, ib_strip_cut_value: u32),
    /// `ID3D12GraphicsCommandList10::SetProgram` (slot 84).
    pub set_program:
        unsafe extern "system-unwind" fn(this: *mut c_void, p_desc: *const SetProgramDesc),
    /// `ID3D12GraphicsCommandList10::DispatchGraph` (slot 85).
    pub dispatch_graph:
        unsafe extern "system-unwind" fn(this: *mut c_void, p_desc: *const DispatchGraphDesc),
}

const _: () = {
    assert!(size_of::<SamplerBorderColor>() == 16 && align_of::<SamplerBorderColor>() == 4);
    assert!(size_of::<SamplerDesc2>() == 56 && align_of::<SamplerDesc2>() == 4);
    assert!(offset_of!(SamplerDesc2, filter) == 0);
    assert!(offset_of!(SamplerDesc2, address_u) == 4);
    assert!(offset_of!(SamplerDesc2, address_v) == 8);
    assert!(offset_of!(SamplerDesc2, address_w) == 12);
    assert!(offset_of!(SamplerDesc2, mip_lod_bias) == 16);
    assert!(offset_of!(SamplerDesc2, max_anisotropy) == 20);
    assert!(offset_of!(SamplerDesc2, comparison_func) == 24);
    assert!(offset_of!(SamplerDesc2, border_color) == 28);
    assert!(offset_of!(SamplerDesc2, min_lod) == 44);
    assert!(offset_of!(SamplerDesc2, max_lod) == 48);
    assert!(offset_of!(SamplerDesc2, flags) == 52);

    assert!(size_of::<ProgramIdentifier>() == 32 && align_of::<ProgramIdentifier>() == 8);
    assert!(size_of::<GpuVirtualAddressRange>() == 16 && align_of::<GpuVirtualAddressRange>() == 8);
    assert!(
        size_of::<GpuVirtualAddressRangeAndStride>() == 24
            && align_of::<GpuVirtualAddressRangeAndStride>() == 8
    );
    assert!(size_of::<SetGenericPipelineDesc>() == 32 && align_of::<SetGenericPipelineDesc>() == 8);
    assert!(
        size_of::<SetRaytracingPipelineDesc>() == 32
            && align_of::<SetRaytracingPipelineDesc>() == 8
    );
    assert!(size_of::<SetWorkGraphDesc>() == 80 && align_of::<SetWorkGraphDesc>() == 8);
    assert!(offset_of!(SetWorkGraphDesc, program_identifier) == 0);
    assert!(offset_of!(SetWorkGraphDesc, flags) == 32);
    assert!(offset_of!(SetWorkGraphDesc, backing_memory) == 40);
    assert!(offset_of!(SetWorkGraphDesc, node_local_root_arguments_table) == 56);
    assert!(size_of::<SetMlirProgramDesc>() == 40 && align_of::<SetMlirProgramDesc>() == 8);
    assert!(size_of::<SetProgramDescUnion>() == 80 && align_of::<SetProgramDescUnion>() == 8);
    assert!(size_of::<SetProgramDesc>() == 88 && align_of::<SetProgramDesc>() == 8);
    assert!(offset_of!(SetProgramDesc, r#type) == 0);
    assert!(offset_of!(SetProgramDesc, u) == 8);

    assert!(size_of::<NodeCpuInput>() == 24 && align_of::<NodeCpuInput>() == 8);
    assert!(offset_of!(NodeCpuInput, entrypoint_index) == 0);
    assert!(offset_of!(NodeCpuInput, num_records) == 4);
    assert!(offset_of!(NodeCpuInput, p_records) == 8);
    assert!(offset_of!(NodeCpuInput, record_stride_in_bytes) == 16);
    assert!(size_of::<DispatchGraphDescUnion>() == 24 && align_of::<DispatchGraphDescUnion>() == 8);
    assert!(size_of::<DispatchGraphDesc>() == 32 && align_of::<DispatchGraphDesc>() == 8);
    assert!(offset_of!(DispatchGraphDesc, mode) == 0);
    assert!(offset_of!(DispatchGraphDesc, u) == 8);

    assert!(offset_of!(ID3D12Device14Vtbl, base) == 0);
    assert!(align_of::<ID3D12Device14Vtbl>() == align_of::<ID3D12Device10Vtbl>());
    assert!(
        size_of::<ID3D12Device14Vtbl>() == size_of::<ID3D12Device10Vtbl>() + 4 * size_of::<usize>()
    );
    assert!(offset_of!(ID3D12Device14Vtbl, create_sampler2) == size_of::<ID3D12Device10Vtbl>());
    assert!(
        offset_of!(ID3D12Device14Vtbl, get_resource_allocation_info3)
            == size_of::<ID3D12Device10Vtbl>() + size_of::<usize>()
    );
    assert!(
        offset_of!(ID3D12Device14Vtbl, open_existing_heap_from_address1)
            == size_of::<ID3D12Device10Vtbl>() + 2 * size_of::<usize>()
    );
    assert!(
        offset_of!(
            ID3D12Device14Vtbl,
            create_root_signature_from_subobject_in_library
        ) == size_of::<ID3D12Device10Vtbl>() + 3 * size_of::<usize>()
    );

    assert!(offset_of!(ID3D12GraphicsCommandList10Vtbl, base) == 0);
    assert!(
        align_of::<ID3D12GraphicsCommandList10Vtbl>()
            == align_of::<ID3D12GraphicsCommandList7Vtbl>()
    );
    assert!(
        size_of::<ID3D12GraphicsCommandList10Vtbl>()
            == size_of::<ID3D12GraphicsCommandList7Vtbl>() + 5 * size_of::<usize>()
    );
    assert!(
        offset_of!(
            ID3D12GraphicsCommandList10Vtbl,
            om_set_front_and_back_stencil_ref
        ) == size_of::<ID3D12GraphicsCommandList7Vtbl>()
    );
    assert!(
        offset_of!(ID3D12GraphicsCommandList10Vtbl, rs_set_depth_bias)
            == size_of::<ID3D12GraphicsCommandList7Vtbl>() + size_of::<usize>()
    );
    assert!(
        offset_of!(
            ID3D12GraphicsCommandList10Vtbl,
            ia_set_index_buffer_strip_cut_value
        ) == size_of::<ID3D12GraphicsCommandList7Vtbl>() + 2 * size_of::<usize>()
    );
    assert!(
        offset_of!(ID3D12GraphicsCommandList10Vtbl, set_program)
            == size_of::<ID3D12GraphicsCommandList7Vtbl>() + 3 * size_of::<usize>()
    );
    assert!(
        offset_of!(ID3D12GraphicsCommandList10Vtbl, dispatch_graph)
            == size_of::<ID3D12GraphicsCommandList7Vtbl>() + 4 * size_of::<usize>()
    );
};

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<MultiNodeCpuInput>() == 24 && align_of::<MultiNodeCpuInput>() == 8);
    assert!(offset_of!(MultiNodeCpuInput, num_node_inputs) == 0);
    assert!(offset_of!(MultiNodeCpuInput, p_node_inputs) == 8);
    assert!(offset_of!(MultiNodeCpuInput, node_input_stride_in_bytes) == 16);
    assert!(size_of::<MlirProgramBindings>() == 16 && align_of::<MlirProgramBindings>() == 8);
    assert!(offset_of!(MlirProgramBindings, num_bindings) == 0);
    assert!(offset_of!(MlirProgramBindings, p_bindings) == 8);
    assert!(size_of::<ID3D12Device14Vtbl>() == 664);
    assert!(size_of::<ID3D12GraphicsCommandList10Vtbl>() == 688);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<MultiNodeCpuInput>() == 16 && align_of::<MultiNodeCpuInput>() == 8);
    assert!(offset_of!(MultiNodeCpuInput, num_node_inputs) == 0);
    assert!(offset_of!(MultiNodeCpuInput, p_node_inputs) == 4);
    assert!(offset_of!(MultiNodeCpuInput, node_input_stride_in_bytes) == 8);
    assert!(size_of::<MlirProgramBindings>() == 8 && align_of::<MlirProgramBindings>() == 4);
    assert!(offset_of!(MlirProgramBindings, num_bindings) == 0);
    assert!(offset_of!(MlirProgramBindings, p_bindings) == 4);
    assert!(size_of::<ID3D12Device14Vtbl>() == 332);
    assert!(size_of::<ID3D12GraphicsCommandList10Vtbl>() == 344);
};
