//! `D3D12_FEATURE` and the structures `CheckFeatureSupport` fills.
//!
//! The structures and constants as `d3d12.h` defines them, and the three options structures
//! newer than the header (`d3d12_feature_data.hpp`, the Agility SDK's layouts). Every enumeration
//! is its `u32` (the C compilers give a header enumeration of non-negative values that type), a
//! `BOOL` its `i32`, and the fields are the C's in snake case. A structure's layout is the C's on
//! every target, asserted at the end against clang's record layouts (llvm-mingw's sysroot for
//! the four PE targets, `include/native` natively).

use core::{ffi::c_void, mem::offset_of};

/// `D3D12_FEATURE_D3D12_OPTIONS`.
pub const D3D12_FEATURE_D3D12_OPTIONS: u32 = 0;
/// `D3D12_FEATURE_ARCHITECTURE`.
pub const D3D12_FEATURE_ARCHITECTURE: u32 = 1;
/// `D3D12_FEATURE_FEATURE_LEVELS`.
pub const D3D12_FEATURE_FEATURE_LEVELS: u32 = 2;
/// `D3D12_FEATURE_FORMAT_SUPPORT`.
pub const D3D12_FEATURE_FORMAT_SUPPORT: u32 = 3;
/// `D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS`.
pub const D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS: u32 = 4;
/// `D3D12_FEATURE_FORMAT_INFO`.
pub const D3D12_FEATURE_FORMAT_INFO: u32 = 5;
/// `D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT`.
pub const D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT: u32 = 6;
/// `D3D12_FEATURE_SHADER_MODEL`.
pub const D3D12_FEATURE_SHADER_MODEL: u32 = 7;
/// `D3D12_FEATURE_D3D12_OPTIONS1`.
pub const D3D12_FEATURE_D3D12_OPTIONS1: u32 = 8;
/// `D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT`.
pub const D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT: u32 = 10;
/// `D3D12_FEATURE_ROOT_SIGNATURE`.
pub const D3D12_FEATURE_ROOT_SIGNATURE: u32 = 12;
/// `D3D12_FEATURE_ARCHITECTURE1`.
pub const D3D12_FEATURE_ARCHITECTURE1: u32 = 16;
/// `D3D12_FEATURE_D3D12_OPTIONS2`.
pub const D3D12_FEATURE_D3D12_OPTIONS2: u32 = 18;
/// `D3D12_FEATURE_SHADER_CACHE`.
pub const D3D12_FEATURE_SHADER_CACHE: u32 = 19;
/// `D3D12_FEATURE_D3D12_OPTIONS3`.
pub const D3D12_FEATURE_D3D12_OPTIONS3: u32 = 21;
/// `D3D12_FEATURE_EXISTING_HEAPS`.
pub const D3D12_FEATURE_EXISTING_HEAPS: u32 = 22;
/// `D3D12_FEATURE_D3D12_OPTIONS4`.
pub const D3D12_FEATURE_D3D12_OPTIONS4: u32 = 23;
/// `D3D12_FEATURE_CROSS_NODE`.
pub const D3D12_FEATURE_CROSS_NODE: u32 = 25;
/// `D3D12_FEATURE_D3D12_OPTIONS5`.
pub const D3D12_FEATURE_D3D12_OPTIONS5: u32 = 27;
/// `D3D12_FEATURE_DISPLAYABLE`.
pub const D3D12_FEATURE_DISPLAYABLE: u32 = 28;
/// `D3D12_FEATURE_D3D12_OPTIONS6`.
pub const D3D12_FEATURE_D3D12_OPTIONS6: u32 = 30;
/// `D3D12_FEATURE_D3D12_OPTIONS7`.
pub const D3D12_FEATURE_D3D12_OPTIONS7: u32 = 32;
/// `D3D12_FEATURE_D3D12_OPTIONS8`.
pub const D3D12_FEATURE_D3D12_OPTIONS8: u32 = 36;
/// `D3D12_FEATURE_D3D12_OPTIONS9`.
pub const D3D12_FEATURE_D3D12_OPTIONS9: u32 = 37;
/// `D3D12_FEATURE_D3D12_OPTIONS10`.
pub const D3D12_FEATURE_D3D12_OPTIONS10: u32 = 39;
/// `D3D12_FEATURE_D3D12_OPTIONS11`.
pub const D3D12_FEATURE_D3D12_OPTIONS11: u32 = 40;
/// `D3D12_FEATURE_D3D12_OPTIONS12`.
pub const D3D12_FEATURE_D3D12_OPTIONS12: u32 = 41;
/// `D3D12_FEATURE_D3D12_OPTIONS13`.
pub const D3D12_FEATURE_D3D12_OPTIONS13: u32 = 42;
/// `D3D12_FEATURE_D3D12_OPTIONS14`.
pub const D3D12_FEATURE_D3D12_OPTIONS14: u32 = 43;
/// `D3D12_FEATURE_D3D12_OPTIONS15`.
pub const D3D12_FEATURE_D3D12_OPTIONS15: u32 = 44;
/// `D3D12_FEATURE_D3D12_OPTIONS16`.
pub const D3D12_FEATURE_D3D12_OPTIONS16: u32 = 45;
/// `D3D12_FEATURE_D3D12_OPTIONS17`.
pub const D3D12_FEATURE_D3D12_OPTIONS17: u32 = 46;
/// `D3D12_FEATURE_D3D12_OPTIONS18`.
pub const D3D12_FEATURE_D3D12_OPTIONS18: u32 = 47;
/// The options structure `d3d12_feature_data.hpp` calls `OPTIONS19`.
pub const D3D12_FEATURE_D3D12_OPTIONS19: u32 = 48;
/// The options structure `d3d12_feature_data.hpp` calls `OPTIONS21`.
pub const D3D12_FEATURE_D3D12_OPTIONS21: u32 = 53;
/// The options structure `d3d12_feature_data.hpp` calls `OPTIONS22`.
pub const D3D12_FEATURE_D3D12_OPTIONS22: u32 = 65;

/// `D3D_FEATURE_LEVEL_11_1`.
pub const D3D_FEATURE_LEVEL_11_1: u32 = 0xb100;
/// `D3D_FEATURE_LEVEL_12_1`.
pub const D3D_FEATURE_LEVEL_12_1: u32 = 0xc100;

/// `D3D_SHADER_MODEL_5_1`.
pub const D3D_SHADER_MODEL_5_1: u32 = 0x51;
/// Shader Model 6.8, which the header's `D3D_SHADER_MODEL` does not name.
pub const D3D_SHADER_MODEL_6_8: u32 = 0x68;

/// `D3D12_SHADER_MIN_PRECISION_SUPPORT_16_BIT`.
pub const D3D12_SHADER_MIN_PRECISION_SUPPORT_16_BIT: u32 = 2;
/// `D3D12_RESOURCE_BINDING_TIER_2`.
pub const D3D12_RESOURCE_BINDING_TIER_2: u32 = 2;
/// `D3D12_RESOURCE_BINDING_TIER_3`.
pub const D3D12_RESOURCE_BINDING_TIER_3: u32 = 3;
/// `D3D12_RESOURCE_HEAP_TIER_2`.
pub const D3D12_RESOURCE_HEAP_TIER_2: u32 = 2;
/// `D3D12_FORMAT_SUPPORT1_BUFFER`.
pub const D3D12_FORMAT_SUPPORT1_BUFFER: u32 = 1;
/// `D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_INPROC_CACHE`.
pub const D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_INPROC_CACHE: u32 = 4;
/// `D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_DISK_CACHE`.
pub const D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_DISK_CACHE: u32 = 8;
/// `D3D12_EXECUTE_INDIRECT_TIER_1_0`.
pub const D3D12_EXECUTE_INDIRECT_TIER_1_0: u32 = 10;
/// `D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION`.
pub const D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION: u32 = 65535;

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS`.
#[repr(C)]
pub struct Options {
    pub double_precision_float_shader_ops: i32,
    pub output_merger_logic_op: i32,
    pub min_precision_support: u32,
    pub tiled_resources_tier: u32,
    pub resource_binding_tier: u32,
    pub ps_specified_stencil_ref_supported: i32,
    pub typed_uav_load_additional_formats: i32,
    pub ro_vs_supported: i32,
    pub conservative_rasterization_tier: u32,
    pub max_gpu_virtual_address_bits_per_resource: u32,
    pub standard_swizzle64_kb_supported: i32,
    pub cross_node_sharing_tier: u32,
    pub cross_adapter_row_major_texture_supported: i32,
    pub vp_and_rt_array_index_from_any_shader_feeding_rasterizer_supported_without_gs_emulation:
        i32,
    pub resource_heap_tier: u32,
}

/// `D3D12_FEATURE_DATA_ARCHITECTURE`.
#[repr(C)]
pub struct Architecture {
    pub node_index: u32,
    pub tile_based_renderer: i32,
    pub uma: i32,
    pub cache_coherent_uma: i32,
}

/// `D3D12_FEATURE_DATA_FEATURE_LEVELS`.
#[repr(C)]
pub struct FeatureLevels {
    pub num_feature_levels: u32,
    pub feature_levels_requested: *const c_void,
    pub max_supported_feature_level: u32,
}

/// `D3D12_FEATURE_DATA_FORMAT_SUPPORT`.
#[repr(C)]
pub struct FormatSupport {
    pub format: u32,
    pub support1: u32,
    pub support2: u32,
}

/// `D3D12_FEATURE_DATA_MULTISAMPLE_QUALITY_LEVELS`.
#[repr(C)]
pub struct MultisampleQualityLevels {
    pub format: u32,
    pub sample_count: u32,
    pub flags: u32,
    pub num_quality_levels: u32,
}

/// `D3D12_FEATURE_DATA_FORMAT_INFO`.
#[repr(C)]
pub struct FormatInfo {
    pub format: u32,
    pub plane_count: u8,
}

/// `D3D12_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT`.
#[repr(C)]
pub struct GpuVirtualAddressSupport {
    pub max_gpu_virtual_address_bits_per_resource: u32,
    pub max_gpu_virtual_address_bits_per_process: u32,
}

/// `D3D12_FEATURE_DATA_SHADER_MODEL`.
#[repr(C)]
pub struct ShaderModel {
    pub highest_shader_model: u32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS1`.
#[repr(C)]
pub struct Options1 {
    pub wave_ops: i32,
    pub wave_lane_count_min: u32,
    pub wave_lane_count_max: u32,
    pub total_lane_count: u32,
    pub expanded_compute_resource_states: i32,
    pub int64_shader_ops: i32,
}

/// `D3D12_FEATURE_DATA_PROTECTED_RESOURCE_SESSION_SUPPORT`.
#[repr(C)]
pub struct ProtectedResourceSessionSupport {
    pub node_index: u32,
    pub support: u32,
}

/// `D3D12_FEATURE_DATA_ROOT_SIGNATURE`.
#[repr(C)]
pub struct RootSignature {
    pub highest_version: u32,
}

/// `D3D12_FEATURE_DATA_ARCHITECTURE1`.
#[repr(C)]
pub struct Architecture1 {
    pub node_index: u32,
    pub tile_based_renderer: i32,
    pub uma: i32,
    pub cache_coherent_uma: i32,
    pub isolated_mmu: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS2`.
#[repr(C)]
pub struct Options2 {
    pub depth_bounds_test_supported: i32,
    pub programmable_sample_positions_tier: u32,
}

/// `D3D12_FEATURE_DATA_SHADER_CACHE`.
#[repr(C)]
pub struct ShaderCache {
    pub support_flags: u32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS3`.
#[repr(C)]
pub struct Options3 {
    pub copy_queue_timestamp_queries_supported: i32,
    pub casting_fully_typed_format_supported: i32,
    pub write_buffer_immediate_support_flags: u32,
    pub view_instancing_tier: u32,
    pub barycentrics_supported: i32,
}

/// `D3D12_FEATURE_DATA_EXISTING_HEAPS`.
#[repr(C)]
pub struct ExistingHeaps {
    pub supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS4`.
#[repr(C)]
pub struct Options4 {
    pub msaa64_kb_aligned_texture_supported: i32,
    pub shared_resource_compatibility_tier: u32,
    pub native16_bit_shader_ops_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS5`.
#[repr(C)]
pub struct Options5 {
    pub srv_only_tiled_resource_tier3: i32,
    pub render_passes_tier: u32,
    pub raytracing_tier: u32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS6`.
#[repr(C)]
pub struct Options6 {
    pub additional_shading_rates_supported: i32,
    pub per_primitive_shading_rate_supported_with_viewport_indexing: i32,
    pub variable_shading_rate_tier: u32,
    pub shading_rate_image_tile_size: u32,
    pub background_processing_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS7`.
#[repr(C)]
pub struct Options7 {
    pub mesh_shader_tier: u32,
    pub sampler_feedback_tier: u32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS8`.
#[repr(C)]
pub struct Options8 {
    pub unaligned_block_textures_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS9`.
#[repr(C)]
pub struct Options9 {
    pub mesh_shader_pipeline_stats_supported: i32,
    pub mesh_shader_supports_full_range_render_target_array_index: i32,
    pub atomic_int64_on_typed_resource_supported: i32,
    pub atomic_int64_on_group_shared_supported: i32,
    pub derivatives_in_mesh_and_amplification_shaders_supported: i32,
    pub wave_mma_tier: u32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS10`.
#[repr(C)]
pub struct Options10 {
    pub variable_rate_shading_sum_combiner_supported: i32,
    pub mesh_shader_per_primitive_shading_rate_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS11`.
#[repr(C)]
pub struct Options11 {
    pub atomic_int64_on_descriptor_heap_resource_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS12`.
#[repr(C)]
pub struct Options12 {
    pub ms_primitives_pipeline_statistic_includes_culled_primitives: u32,
    pub enhanced_barriers_supported: i32,
    pub relaxed_format_casting_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS13`.
#[repr(C)]
pub struct Options13 {
    pub unrestricted_buffer_texture_copy_pitch_supported: i32,
    pub unrestricted_vertex_element_alignment_supported: i32,
    pub inverted_viewport_height_flips_y_supported: i32,
    pub inverted_viewport_depth_flips_z_supported: i32,
    pub texture_copy_between_dimensions_supported: i32,
    pub alpha_blend_factor_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS14`.
#[repr(C)]
pub struct Options14 {
    pub advanced_texture_ops_supported: i32,
    pub writeable_msaa_textures_supported: i32,
    pub independent_front_and_back_stencil_ref_mask_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS15`.
#[repr(C)]
pub struct Options15 {
    pub triangle_fan_supported: i32,
    pub dynamic_index_buffer_strip_cut_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS16`.
#[repr(C)]
pub struct Options16 {
    pub dynamic_depth_bias_supported: i32,
    pub gpu_upload_heap_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS17`.
#[repr(C)]
pub struct Options17 {
    pub non_normalized_coordinate_samplers_supported: i32,
    pub manual_write_tracking_resource_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS18`.
#[repr(C)]
pub struct Options18 {
    pub render_passes_valid: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS19_MN`.
#[repr(C)]
pub struct Options19 {
    pub mismatching_output_dimensions_supported: i32,
    pub supported_sample_counts_with_no_outputs: u32,
    pub point_sampling_addresses_never_round_up: i32,
    pub rasterizer_desc2_supported: i32,
    pub narrow_quadrilateral_lines_supported: i32,
    pub aniso_filter_with_point_mip_supported: i32,
    pub max_sampler_descriptor_heap_size: u32,
    pub max_sampler_descriptor_heap_size_with_static_samplers: u32,
    pub max_view_descriptor_heap_size: u32,
    pub compute_only_custom_heap_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS21_MN`.
#[repr(C)]
pub struct Options21 {
    pub work_graphs_tier: u32,
    pub execute_indirect_tier: u32,
    pub sample_cmp_gradient_and_bias_supported: i32,
    pub extended_command_info_supported: i32,
}

/// `D3D12_FEATURE_DATA_D3D12_OPTIONS22_MN`.
#[repr(C)]
pub struct Options22 {
    pub shader_execution_reordering_actually_reorders: i32,
    pub create_byte_offset_views_supported: i32,
    pub max1_d_dispatch_size: u32,
    pub max1_d_dispatch_mesh_size: u32,
}

// The layouts of clang's `-fdump-record-layouts` over the headers named above: every mirrored
// structure's size, alignment and field offsets, the same on every target for
// all but `FeatureLevels`, whose 32-bit block is i686 PE's and
// whose 64-bit one is the other PE targets' and darwin's.
const _: () = {
    assert!(size_of::<Options>() == 60 && align_of::<Options>() == 4);
    assert!(offset_of!(Options, double_precision_float_shader_ops) == 0);
    assert!(offset_of!(Options, output_merger_logic_op) == 4);
    assert!(offset_of!(Options, min_precision_support) == 8);
    assert!(offset_of!(Options, tiled_resources_tier) == 12);
    assert!(offset_of!(Options, resource_binding_tier) == 16);
    assert!(offset_of!(Options, ps_specified_stencil_ref_supported) == 20);
    assert!(offset_of!(Options, typed_uav_load_additional_formats) == 24);
    assert!(offset_of!(Options, ro_vs_supported) == 28);
    assert!(offset_of!(Options, conservative_rasterization_tier) == 32);
    assert!(offset_of!(Options, max_gpu_virtual_address_bits_per_resource) == 36);
    assert!(offset_of!(Options, standard_swizzle64_kb_supported) == 40);
    assert!(offset_of!(Options, cross_node_sharing_tier) == 44);
    assert!(offset_of!(Options, cross_adapter_row_major_texture_supported) == 48);
    assert!(
        offset_of!(
            Options,
            vp_and_rt_array_index_from_any_shader_feeding_rasterizer_supported_without_gs_emulation
        ) == 52
    );
    assert!(offset_of!(Options, resource_heap_tier) == 56);
    assert!(size_of::<Architecture>() == 16 && align_of::<Architecture>() == 4);
    assert!(offset_of!(Architecture, node_index) == 0);
    assert!(offset_of!(Architecture, tile_based_renderer) == 4);
    assert!(offset_of!(Architecture, uma) == 8);
    assert!(offset_of!(Architecture, cache_coherent_uma) == 12);
    assert!(size_of::<FormatSupport>() == 12 && align_of::<FormatSupport>() == 4);
    assert!(offset_of!(FormatSupport, format) == 0);
    assert!(offset_of!(FormatSupport, support1) == 4);
    assert!(offset_of!(FormatSupport, support2) == 8);
    assert!(
        size_of::<MultisampleQualityLevels>() == 16 && align_of::<MultisampleQualityLevels>() == 4
    );
    assert!(offset_of!(MultisampleQualityLevels, format) == 0);
    assert!(offset_of!(MultisampleQualityLevels, sample_count) == 4);
    assert!(offset_of!(MultisampleQualityLevels, flags) == 8);
    assert!(offset_of!(MultisampleQualityLevels, num_quality_levels) == 12);
    assert!(size_of::<FormatInfo>() == 8 && align_of::<FormatInfo>() == 4);
    assert!(offset_of!(FormatInfo, format) == 0);
    assert!(offset_of!(FormatInfo, plane_count) == 4);
    assert!(
        size_of::<GpuVirtualAddressSupport>() == 8 && align_of::<GpuVirtualAddressSupport>() == 4
    );
    assert!(
        offset_of!(
            GpuVirtualAddressSupport,
            max_gpu_virtual_address_bits_per_resource
        ) == 0
    );
    assert!(
        offset_of!(
            GpuVirtualAddressSupport,
            max_gpu_virtual_address_bits_per_process
        ) == 4
    );
    assert!(size_of::<ShaderModel>() == 4 && align_of::<ShaderModel>() == 4);
    assert!(offset_of!(ShaderModel, highest_shader_model) == 0);
    assert!(size_of::<Options1>() == 24 && align_of::<Options1>() == 4);
    assert!(offset_of!(Options1, wave_ops) == 0);
    assert!(offset_of!(Options1, wave_lane_count_min) == 4);
    assert!(offset_of!(Options1, wave_lane_count_max) == 8);
    assert!(offset_of!(Options1, total_lane_count) == 12);
    assert!(offset_of!(Options1, expanded_compute_resource_states) == 16);
    assert!(offset_of!(Options1, int64_shader_ops) == 20);
    assert!(
        size_of::<ProtectedResourceSessionSupport>() == 8
            && align_of::<ProtectedResourceSessionSupport>() == 4
    );
    assert!(offset_of!(ProtectedResourceSessionSupport, node_index) == 0);
    assert!(offset_of!(ProtectedResourceSessionSupport, support) == 4);
    assert!(size_of::<RootSignature>() == 4 && align_of::<RootSignature>() == 4);
    assert!(offset_of!(RootSignature, highest_version) == 0);
    assert!(size_of::<Architecture1>() == 20 && align_of::<Architecture1>() == 4);
    assert!(offset_of!(Architecture1, node_index) == 0);
    assert!(offset_of!(Architecture1, tile_based_renderer) == 4);
    assert!(offset_of!(Architecture1, uma) == 8);
    assert!(offset_of!(Architecture1, cache_coherent_uma) == 12);
    assert!(offset_of!(Architecture1, isolated_mmu) == 16);
    assert!(size_of::<Options2>() == 8 && align_of::<Options2>() == 4);
    assert!(offset_of!(Options2, depth_bounds_test_supported) == 0);
    assert!(offset_of!(Options2, programmable_sample_positions_tier) == 4);
    assert!(size_of::<ShaderCache>() == 4 && align_of::<ShaderCache>() == 4);
    assert!(offset_of!(ShaderCache, support_flags) == 0);
    assert!(size_of::<Options3>() == 20 && align_of::<Options3>() == 4);
    assert!(offset_of!(Options3, copy_queue_timestamp_queries_supported) == 0);
    assert!(offset_of!(Options3, casting_fully_typed_format_supported) == 4);
    assert!(offset_of!(Options3, write_buffer_immediate_support_flags) == 8);
    assert!(offset_of!(Options3, view_instancing_tier) == 12);
    assert!(offset_of!(Options3, barycentrics_supported) == 16);
    assert!(size_of::<ExistingHeaps>() == 4 && align_of::<ExistingHeaps>() == 4);
    assert!(offset_of!(ExistingHeaps, supported) == 0);
    assert!(size_of::<Options4>() == 12 && align_of::<Options4>() == 4);
    assert!(offset_of!(Options4, msaa64_kb_aligned_texture_supported) == 0);
    assert!(offset_of!(Options4, shared_resource_compatibility_tier) == 4);
    assert!(offset_of!(Options4, native16_bit_shader_ops_supported) == 8);
    assert!(size_of::<Options5>() == 12 && align_of::<Options5>() == 4);
    assert!(offset_of!(Options5, srv_only_tiled_resource_tier3) == 0);
    assert!(offset_of!(Options5, render_passes_tier) == 4);
    assert!(offset_of!(Options5, raytracing_tier) == 8);
    assert!(size_of::<Options6>() == 20 && align_of::<Options6>() == 4);
    assert!(offset_of!(Options6, additional_shading_rates_supported) == 0);
    assert!(
        offset_of!(
            Options6,
            per_primitive_shading_rate_supported_with_viewport_indexing
        ) == 4
    );
    assert!(offset_of!(Options6, variable_shading_rate_tier) == 8);
    assert!(offset_of!(Options6, shading_rate_image_tile_size) == 12);
    assert!(offset_of!(Options6, background_processing_supported) == 16);
    assert!(size_of::<Options7>() == 8 && align_of::<Options7>() == 4);
    assert!(offset_of!(Options7, mesh_shader_tier) == 0);
    assert!(offset_of!(Options7, sampler_feedback_tier) == 4);
    assert!(size_of::<Options8>() == 4 && align_of::<Options8>() == 4);
    assert!(offset_of!(Options8, unaligned_block_textures_supported) == 0);
    assert!(size_of::<Options9>() == 24 && align_of::<Options9>() == 4);
    assert!(offset_of!(Options9, mesh_shader_pipeline_stats_supported) == 0);
    assert!(
        offset_of!(
            Options9,
            mesh_shader_supports_full_range_render_target_array_index
        ) == 4
    );
    assert!(offset_of!(Options9, atomic_int64_on_typed_resource_supported) == 8);
    assert!(offset_of!(Options9, atomic_int64_on_group_shared_supported) == 12);
    assert!(
        offset_of!(
            Options9,
            derivatives_in_mesh_and_amplification_shaders_supported
        ) == 16
    );
    assert!(offset_of!(Options9, wave_mma_tier) == 20);
    assert!(size_of::<Options10>() == 8 && align_of::<Options10>() == 4);
    assert!(offset_of!(Options10, variable_rate_shading_sum_combiner_supported) == 0);
    assert!(offset_of!(Options10, mesh_shader_per_primitive_shading_rate_supported) == 4);
    assert!(size_of::<Options11>() == 4 && align_of::<Options11>() == 4);
    assert!(
        offset_of!(
            Options11,
            atomic_int64_on_descriptor_heap_resource_supported
        ) == 0
    );
    assert!(size_of::<Options12>() == 12 && align_of::<Options12>() == 4);
    assert!(
        offset_of!(
            Options12,
            ms_primitives_pipeline_statistic_includes_culled_primitives
        ) == 0
    );
    assert!(offset_of!(Options12, enhanced_barriers_supported) == 4);
    assert!(offset_of!(Options12, relaxed_format_casting_supported) == 8);
    assert!(size_of::<Options13>() == 24 && align_of::<Options13>() == 4);
    assert!(offset_of!(Options13, unrestricted_buffer_texture_copy_pitch_supported) == 0);
    assert!(offset_of!(Options13, unrestricted_vertex_element_alignment_supported) == 4);
    assert!(offset_of!(Options13, inverted_viewport_height_flips_y_supported) == 8);
    assert!(offset_of!(Options13, inverted_viewport_depth_flips_z_supported) == 12);
    assert!(offset_of!(Options13, texture_copy_between_dimensions_supported) == 16);
    assert!(offset_of!(Options13, alpha_blend_factor_supported) == 20);
    assert!(size_of::<Options14>() == 12 && align_of::<Options14>() == 4);
    assert!(offset_of!(Options14, advanced_texture_ops_supported) == 0);
    assert!(offset_of!(Options14, writeable_msaa_textures_supported) == 4);
    assert!(
        offset_of!(
            Options14,
            independent_front_and_back_stencil_ref_mask_supported
        ) == 8
    );
    assert!(size_of::<Options15>() == 8 && align_of::<Options15>() == 4);
    assert!(offset_of!(Options15, triangle_fan_supported) == 0);
    assert!(offset_of!(Options15, dynamic_index_buffer_strip_cut_supported) == 4);
    assert!(size_of::<Options16>() == 8 && align_of::<Options16>() == 4);
    assert!(offset_of!(Options16, dynamic_depth_bias_supported) == 0);
    assert!(offset_of!(Options16, gpu_upload_heap_supported) == 4);
    assert!(size_of::<Options17>() == 8 && align_of::<Options17>() == 4);
    assert!(offset_of!(Options17, non_normalized_coordinate_samplers_supported) == 0);
    assert!(offset_of!(Options17, manual_write_tracking_resource_supported) == 4);
    assert!(size_of::<Options18>() == 4 && align_of::<Options18>() == 4);
    assert!(offset_of!(Options18, render_passes_valid) == 0);
    assert!(size_of::<Options19>() == 40 && align_of::<Options19>() == 4);
    assert!(offset_of!(Options19, mismatching_output_dimensions_supported) == 0);
    assert!(offset_of!(Options19, supported_sample_counts_with_no_outputs) == 4);
    assert!(offset_of!(Options19, point_sampling_addresses_never_round_up) == 8);
    assert!(offset_of!(Options19, rasterizer_desc2_supported) == 12);
    assert!(offset_of!(Options19, narrow_quadrilateral_lines_supported) == 16);
    assert!(offset_of!(Options19, aniso_filter_with_point_mip_supported) == 20);
    assert!(offset_of!(Options19, max_sampler_descriptor_heap_size) == 24);
    assert!(
        offset_of!(
            Options19,
            max_sampler_descriptor_heap_size_with_static_samplers
        ) == 28
    );
    assert!(offset_of!(Options19, max_view_descriptor_heap_size) == 32);
    assert!(offset_of!(Options19, compute_only_custom_heap_supported) == 36);
    assert!(size_of::<Options21>() == 16 && align_of::<Options21>() == 4);
    assert!(offset_of!(Options21, work_graphs_tier) == 0);
    assert!(offset_of!(Options21, execute_indirect_tier) == 4);
    assert!(offset_of!(Options21, sample_cmp_gradient_and_bias_supported) == 8);
    assert!(offset_of!(Options21, extended_command_info_supported) == 12);
    assert!(size_of::<Options22>() == 16 && align_of::<Options22>() == 4);
    assert!(offset_of!(Options22, shader_execution_reordering_actually_reorders) == 0);
    assert!(offset_of!(Options22, create_byte_offset_views_supported) == 4);
    assert!(offset_of!(Options22, max1_d_dispatch_size) == 8);
    assert!(offset_of!(Options22, max1_d_dispatch_mesh_size) == 12);
};
#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<FeatureLevels>() == 12 && align_of::<FeatureLevels>() == 4);
    assert!(offset_of!(FeatureLevels, num_feature_levels) == 0);
    assert!(offset_of!(FeatureLevels, feature_levels_requested) == 4);
    assert!(offset_of!(FeatureLevels, max_supported_feature_level) == 8);
};
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<FeatureLevels>() == 24 && align_of::<FeatureLevels>() == 8);
    assert!(offset_of!(FeatureLevels, num_feature_levels) == 0);
    assert!(offset_of!(FeatureLevels, feature_levels_requested) == 8);
    assert!(offset_of!(FeatureLevels, max_supported_feature_level) == 16);
};
