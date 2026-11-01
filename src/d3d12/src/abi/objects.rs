//! D3D12 ABI types for descriptor heaps, fences, heaps, queries, and command signatures.

use core::mem::ManuallyDrop;

use crate::abi::resource::HeapProperties;

/// `D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV: u32 = 0;
/// `D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER: u32 = 1;
/// `D3D12_DESCRIPTOR_HEAP_TYPE_RTV` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_TYPE_RTV: u32 = 2;
/// `D3D12_DESCRIPTOR_HEAP_TYPE_DSV` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_TYPE_DSV: u32 = 3;
/// `D3D12_DESCRIPTOR_HEAP_TYPE_NUM_TYPES` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_TYPE_NUM_TYPES: u32 = 4;

/// `D3D12_DESCRIPTOR_HEAP_FLAG_NONE` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_FLAG_NONE: u32 = 0;
/// `D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE` (`d3d12.idl`).
pub const D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE: u32 = 0x1;

/// `D3D12_DESCRIPTOR_HEAP_DESC` (`d3d12.idl`, size 16, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DescriptorHeapDesc {
    pub r#type: u32,
    pub num_descriptors: u32,
    pub flags: u32,
    pub node_mask: u32,
}

impl DescriptorHeapDesc {
    /// Copies the descriptor fields by value.
    #[must_use]
    pub const fn copy(&self) -> Self {
        Self {
            r#type: self.r#type,
            num_descriptors: self.num_descriptors,
            flags: self.flags,
            node_mask: self.node_mask,
        }
    }
}

/// `D3D12_SRV_DIMENSION_UNKNOWN` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_SRV_DIMENSION_BUFFER` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_SRV_DIMENSION_TEXTURE1D` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_SRV_DIMENSION_TEXTURE1DARRAY` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
/// `D3D12_SRV_DIMENSION_TEXTURE2D` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE2D: u32 = 4;
/// `D3D12_SRV_DIMENSION_TEXTURE2DARRAY` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
/// `D3D12_SRV_DIMENSION_TEXTURE2DMS` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE2DMS: u32 = 6;
/// `D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
/// `D3D12_SRV_DIMENSION_TEXTURE3D` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURE3D: u32 = 8;
/// `D3D12_SRV_DIMENSION_TEXTURECUBE` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURECUBE: u32 = 9;
/// `D3D12_SRV_DIMENSION_TEXTURECUBEARRAY` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_TEXTURECUBEARRAY: u32 = 10;
/// `D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE` (`d3d12.idl`).
pub const D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE: u32 = 11;

/// `D3D12_UAV_DIMENSION_UNKNOWN` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D12_UAV_DIMENSION_BUFFER` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_UAV_DIMENSION_TEXTURE1D` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_UAV_DIMENSION_TEXTURE1DARRAY` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
/// `D3D12_UAV_DIMENSION_TEXTURE2D` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE2D: u32 = 4;
/// `D3D12_UAV_DIMENSION_TEXTURE2DARRAY` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
/// `D3D12_UAV_DIMENSION_TEXTURE2DMS` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE2DMS: u32 = 6;
/// `D3D12_UAV_DIMENSION_TEXTURE2DMSARRAY` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
/// `D3D12_UAV_DIMENSION_TEXTURE3D` (`d3d12.idl`).
pub const D3D12_UAV_DIMENSION_TEXTURE3D: u32 = 8;

/// `D3D12_BUFFER_SRV_FLAG_NONE` (`d3d12.idl`).
pub const D3D12_BUFFER_SRV_FLAG_NONE: u32 = 0;
/// `D3D12_BUFFER_SRV_FLAG_RAW` (`d3d12.idl`).
pub const D3D12_BUFFER_SRV_FLAG_RAW: u32 = 0x1;

/// `D3D12_BUFFER_UAV_FLAG_NONE` (`d3d12.idl`).
pub const D3D12_BUFFER_UAV_FLAG_NONE: u32 = 0;
/// `D3D12_BUFFER_UAV_FLAG_RAW` (`d3d12.idl`).
pub const D3D12_BUFFER_UAV_FLAG_RAW: u32 = 0x1;

/// `D3D12_BUFFER_SRV` (`d3d12.idl`, size 24, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct BufferSrv {
    pub first_element: u64,
    pub num_elements: u32,
    pub structure_byte_stride: u32,
    pub flags: u32,
}

/// Prefix of `D3D12_SHADER_RESOURCE_VIEW_DESC` with `Buffer` (`d3d12.idl`, size 40, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ShaderResourceViewDescPrefix {
    pub format: u32,
    pub view_dimension: u32,
    pub shader_4_component_mapping: u32,
    pub buffer: BufferSrv,
}

/// `D3D12_BUFFER_UAV` (`d3d12.idl`, size 32, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct BufferUav {
    pub first_element: u64,
    pub num_elements: u32,
    pub structure_byte_stride: u32,
    pub counter_offset_in_bytes: u64,
    pub flags: u32,
}

/// Prefix of `D3D12_UNORDERED_ACCESS_VIEW_DESC` with `Buffer` (`d3d12.idl`, size 40, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct UnorderedAccessViewDescPrefix {
    pub format: u32,
    pub view_dimension: u32,
    pub buffer: BufferUav,
}

/// `SRVTextureGPUStorage` (`d3d12_descriptor_heap.hpp:23`, size 32, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SrvTextureGpuStorage {
    pub resource_id: u64,
    pub metadata: u64,
    pub atomic_resource_id: u64,
    pub padding: u64,
}

impl SrvTextureGpuStorage {
    /// Packs the descriptor into four 64-bit GPU words.
    #[must_use]
    pub const fn to_words(&self) -> [u64; 4] {
        [
            self.resource_id,
            self.metadata,
            self.atomic_resource_id,
            self.padding,
        ]
    }
}

/// `UAVTextureGPUStorage` (`d3d12_descriptor_heap.hpp:30`, size 32, align 8).
pub type UavTextureGpuStorage = SrvTextureGpuStorage;

/// `UAVTexelBufferGPUStorage` (`d3d12_descriptor_heap.hpp:32`, size 32, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct UavTexelBufferGpuStorage {
    pub resource_id: u64,
    pub metadata: u64,
    pub atomic_resource_id: u64,
    pub buffer_address: u64,
}

impl UavTexelBufferGpuStorage {
    /// Packs the descriptor into four 64-bit GPU words.
    #[must_use]
    pub const fn to_words(&self) -> [u64; 4] {
        [
            self.resource_id,
            self.metadata,
            self.atomic_resource_id,
            self.buffer_address,
        ]
    }
}

/// `SRVTexelBufferGPUStorage` (`d3d12_descriptor_heap.hpp:39`, size 32, align 8).
pub type SrvTexelBufferGpuStorage = UavTexelBufferGpuStorage;

/// `UAVBufferGPUStorage` (`d3d12_descriptor_heap.hpp:41`, size 32, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct UavBufferGpuStorage {
    pub pointer: u64,
    pub metadata: u64,
    pub counter_pointer: u64,
    pub padding: u64,
}

impl UavBufferGpuStorage {
    /// Packs the descriptor into four 64-bit GPU words.
    #[must_use]
    pub const fn to_words(&self) -> [u64; 4] {
        [
            self.pointer,
            self.metadata,
            self.counter_pointer,
            self.padding,
        ]
    }
}

/// `SRVBufferGPUStorage` (`d3d12_descriptor_heap.hpp:48`, size 32, align 8).
pub type SrvBufferGpuStorage = UavBufferGpuStorage;

/// `CBVCommonStorage` (`d3d12_descriptor_heap.hpp:50`, size 16, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CbvCommonStorage {
    pub address: u64,
    pub size: u64,
}

impl CbvCommonStorage {
    /// Packs the descriptor into four 64-bit GPU words (zero-padded to 32 bytes).
    #[must_use]
    pub const fn to_words(&self) -> [u64; 4] {
        [self.address, self.size, 0, 0]
    }
}

/// `SamplerGPUStorage` (`d3d12_descriptor_heap.hpp:55`, size 32, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SamplerGpuStorage {
    pub sampler: u64,
    pub cube_sampler: u64,
    pub metadata: u64,
    pub padding: u64,
}

impl SamplerGpuStorage {
    /// Packs the descriptor into four 64-bit GPU words.
    #[must_use]
    pub const fn to_words(&self) -> [u64; 4] {
        [self.sampler, self.cube_sampler, self.metadata, self.padding]
    }
}

/// `D3D12_FENCE_FLAG_NONE` (`d3d12.idl`).
pub const D3D12_FENCE_FLAG_NONE: u32 = 0;
/// `D3D12_FENCE_FLAG_SHARED` (`d3d12.idl`).
pub const D3D12_FENCE_FLAG_SHARED: u32 = 0x1;
/// `D3D12_FENCE_FLAG_SHARED_CROSS_ADAPTER` (`d3d12.idl`).
pub const D3D12_FENCE_FLAG_SHARED_CROSS_ADAPTER: u32 = 0x2;
/// `D3D12_FENCE_FLAG_NON_MONITORED` (`d3d12.idl`).
pub const D3D12_FENCE_FLAG_NON_MONITORED: u32 = 0x4;

/// `D3D12_MULTIPLE_FENCE_WAIT_FLAG_NONE` (`d3d12.idl`).
pub const D3D12_MULTIPLE_FENCE_WAIT_FLAG_NONE: u32 = 0;
/// `D3D12_MULTIPLE_FENCE_WAIT_FLAG_ANY` (`d3d12.idl`).
pub const D3D12_MULTIPLE_FENCE_WAIT_FLAG_ANY: u32 = 0x1;
/// `D3D12_MULTIPLE_FENCE_WAIT_FLAG_ALL` (`d3d12.idl`).
pub const D3D12_MULTIPLE_FENCE_WAIT_FLAG_ALL: u32 = 0;

/// `D3D12_HEAP_FLAG_ALLOW_DISPLAY` (`d3d12.idl`).
pub const D3D12_HEAP_FLAG_ALLOW_DISPLAY: u32 = 0x8;

/// `D3D12_HEAP_DESC` (`d3d12.idl`, size 48, align 8).
#[repr(C)]
pub struct HeapDesc {
    pub size_in_bytes: u64,
    pub properties: HeapProperties,
    pub alignment: u64,
    pub flags: u32,
}

impl HeapDesc {
    /// Zeroed heap descriptor.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            size_in_bytes: 0,
            properties: HeapProperties {
                r#type: 0,
                cpu_page_property: 0,
                memory_pool_preference: 0,
                creation_node_mask: 0,
                visible_node_mask: 0,
            },
            alignment: 0,
            flags: 0,
        }
    }

    /// Copies the heap descriptor by value.
    #[must_use]
    pub const fn copy(&self) -> Self {
        Self {
            size_in_bytes: self.size_in_bytes,
            properties: HeapProperties {
                r#type: self.properties.r#type,
                cpu_page_property: self.properties.cpu_page_property,
                memory_pool_preference: self.properties.memory_pool_preference,
                creation_node_mask: self.properties.creation_node_mask,
                visible_node_mask: self.properties.visible_node_mask,
            },
            alignment: self.alignment,
            flags: self.flags,
        }
    }
}

/// `D3D12_QUERY_HEAP_TYPE_OCCLUSION` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_OCCLUSION: u32 = 0;
/// `D3D12_QUERY_HEAP_TYPE_TIMESTAMP` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_TIMESTAMP: u32 = 1;
/// `D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS: u32 = 2;
/// `D3D12_QUERY_HEAP_TYPE_SO_STATISTICS` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_SO_STATISTICS: u32 = 3;
/// `D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS: u32 = 4;
/// `D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP: u32 = 5;
/// `D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1` (`d3d12.idl`).
pub const D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1: u32 = 7;

/// `D3D12_QUERY_HEAP_DESC` (`d3d12.idl`, size 12, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct QueryHeapDesc {
    pub r#type: u32,
    pub count: u32,
    pub node_mask: u32,
}

impl QueryHeapDesc {
    /// Copies the query heap descriptor by value.
    #[must_use]
    pub const fn copy(&self) -> Self {
        Self {
            r#type: self.r#type,
            count: self.count,
            node_mask: self.node_mask,
        }
    }
}

/// `D3D12_QUERY_DATA_PIPELINE_STATISTICS` (`d3d12.idl`, size 88, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct QueryDataPipelineStatistics {
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

/// `D3D12_QUERY_DATA_SO_STATISTICS` (`d3d12.idl`, size 16, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct QueryDataSoStatistics {
    pub num_primitives_written: u64,
    pub primitives_storage_needed: u64,
}

/// `D3D12_INDIRECT_ARGUMENT_TYPE_DRAW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DRAW: u32 = 0;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED: u32 = 1;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH: u32 = 2;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW: u32 = 3;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW: u32 = 4;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT: u32 = 5;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW: u32 = 6;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW: u32 = 7;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW: u32 = 8;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS: u32 = 9;
/// `D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH` (`d3d12.idl`).
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH: u32 = 10;

/// `D3D12_DRAW_ARGUMENTS` (`d3d12.idl`, size 16, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DrawArguments {
    pub vertex_count_per_instance: u32,
    pub instance_count: u32,
    pub start_vertex_location: u32,
    pub start_instance_location: u32,
}

/// `D3D12_DRAW_INDEXED_ARGUMENTS` (`d3d12.idl`, size 20, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DrawIndexedArguments {
    pub index_count_per_instance: u32,
    pub instance_count: u32,
    pub start_index_location: u32,
    pub base_vertex_location: i32,
    pub start_instance_location: u32,
}

/// `D3D12_DISPATCH_ARGUMENTS` (`d3d12.idl`, size 12, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DispatchArguments {
    pub thread_group_count_x: u32,
    pub thread_group_count_y: u32,
    pub thread_group_count_z: u32,
}

/// `D3D12_VERTEX_BUFFER_VIEW` (`d3d12.idl`, size 16, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct VertexBufferView {
    pub buffer_location: u64,
    pub size_in_bytes: u32,
    pub stride_in_bytes: u32,
}

/// `D3D12_INDEX_BUFFER_VIEW` (`d3d12.idl`, size 16, align 8).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IndexBufferView {
    pub buffer_location: u64,
    pub size_in_bytes: u32,
    pub format: u32,
}

/// `VertexBuffer` arm of `D3D12_INDIRECT_ARGUMENT_DESC` (`d3d12.idl`, size 4, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IndirectArgumentVertexBuffer {
    pub slot: u32,
}

/// `Constant` arm of `D3D12_INDIRECT_ARGUMENT_DESC` (`d3d12.idl`, size 12, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IndirectArgumentConstant {
    pub root_parameter_index: u32,
    pub dest_offset_in_32_bit_values: u32,
    pub num_32_bit_values_to_set: u32,
}

/// Root-view arm of `D3D12_INDIRECT_ARGUMENT_DESC` (`d3d12.idl`, size 4, align 4).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IndirectArgumentRootView {
    pub root_parameter_index: u32,
}

/// Union body of `D3D12_INDIRECT_ARGUMENT_DESC` (`d3d12.idl`, size 12, align 4).
#[repr(C)]
pub union IndirectArgumentDescUnion {
    pub vertex_buffer: ManuallyDrop<IndirectArgumentVertexBuffer>,
    pub constant: ManuallyDrop<IndirectArgumentConstant>,
    pub constant_buffer_view: ManuallyDrop<IndirectArgumentRootView>,
    pub shader_resource_view: ManuallyDrop<IndirectArgumentRootView>,
    pub unordered_access_view: ManuallyDrop<IndirectArgumentRootView>,
}

/// `D3D12_INDIRECT_ARGUMENT_DESC` (`d3d12.idl`, size 16, align 4).
#[repr(C)]
pub struct IndirectArgumentDesc {
    pub r#type: u32,
    pub u: IndirectArgumentDescUnion,
}

/// `D3D12_COMMAND_SIGNATURE_DESC` (`d3d12.idl`, size 24 on 64-bit / 16 on 32-bit).
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CommandSignatureDesc {
    pub byte_stride: u32,
    pub num_argument_descs: u32,
    pub p_argument_descs: *const IndirectArgumentDesc,
    pub node_mask: u32,
}
