//! `CheckFeatureSupport`'s answers: what the device reports for each `D3D12_FEATURE`.
//!
//! Ported from `ID3D12Device::CheckFeatureSupport` (`d3d12_device.cpp`), for every feature and
//! every buffer size with the C++'s result. The application's buffer is bytes with no alignment
//! promised: a feature's structure is read and written field by field at its offsets, and a
//! field the C++ leaves alone stays as the application had it, padding included. A buffer of
//! the wrong size for a handled feature is `E_INVALIDARG`; an unhandled feature is `E_NOTIMPL`,
//! which the C++ also logs (the front end does, it has the feature).
//!
//! What the device decides is the three methods of [`Device`].

use core::mem::{offset_of, size_of};

use dxmt_core::format::{E_FAIL, query_dxgi};
use dxmt_util::com::Hresult;

use crate::abi::{
    E_INVALIDARG,
    feature::{
        Architecture, Architecture1, D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_12_1,
        D3D_SHADER_MODEL_5_1, D3D_SHADER_MODEL_6_8,
        D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION, D3D12_EXECUTE_INDIRECT_TIER_1_0,
        D3D12_FEATURE_ARCHITECTURE, D3D12_FEATURE_ARCHITECTURE1, D3D12_FEATURE_CROSS_NODE,
        D3D12_FEATURE_D3D12_OPTIONS, D3D12_FEATURE_D3D12_OPTIONS1, D3D12_FEATURE_D3D12_OPTIONS2,
        D3D12_FEATURE_D3D12_OPTIONS3, D3D12_FEATURE_D3D12_OPTIONS4, D3D12_FEATURE_D3D12_OPTIONS5,
        D3D12_FEATURE_D3D12_OPTIONS6, D3D12_FEATURE_D3D12_OPTIONS7, D3D12_FEATURE_D3D12_OPTIONS8,
        D3D12_FEATURE_D3D12_OPTIONS9, D3D12_FEATURE_D3D12_OPTIONS10, D3D12_FEATURE_D3D12_OPTIONS11,
        D3D12_FEATURE_D3D12_OPTIONS12, D3D12_FEATURE_D3D12_OPTIONS13,
        D3D12_FEATURE_D3D12_OPTIONS14, D3D12_FEATURE_D3D12_OPTIONS15,
        D3D12_FEATURE_D3D12_OPTIONS16, D3D12_FEATURE_D3D12_OPTIONS17,
        D3D12_FEATURE_D3D12_OPTIONS18, D3D12_FEATURE_D3D12_OPTIONS19,
        D3D12_FEATURE_D3D12_OPTIONS21, D3D12_FEATURE_D3D12_OPTIONS22, D3D12_FEATURE_DISPLAYABLE,
        D3D12_FEATURE_EXISTING_HEAPS, D3D12_FEATURE_FEATURE_LEVELS, D3D12_FEATURE_FORMAT_INFO,
        D3D12_FEATURE_FORMAT_SUPPORT, D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT,
        D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS, D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT,
        D3D12_FEATURE_ROOT_SIGNATURE, D3D12_FEATURE_SHADER_CACHE, D3D12_FEATURE_SHADER_MODEL,
        D3D12_FORMAT_SUPPORT1_BUFFER, D3D12_RESOURCE_BINDING_TIER_2, D3D12_RESOURCE_BINDING_TIER_3,
        D3D12_RESOURCE_HEAP_TIER_2, D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_DISK_CACHE,
        D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_INPROC_CACHE,
        D3D12_SHADER_MIN_PRECISION_SUPPORT_16_BIT, ExistingHeaps, FeatureLevels, FormatInfo,
        FormatSupport, GpuVirtualAddressSupport, MultisampleQualityLevels, Options, Options1,
        Options2, Options3, Options4, Options5, Options6, Options7, Options8, Options9, Options10,
        Options11, Options12, Options13, Options14, Options15, Options16, Options17, Options18,
        Options19, Options21, Options22, ProtectedResourceSessionSupport, RootSignature,
        ShaderCache, ShaderModel,
    },
    pipeline::{E_NOTIMPL, S_OK},
    root_signature::{D3D_ROOT_SIGNATURE_VERSION_1_0, D3D_ROOT_SIGNATURE_VERSION_1_1},
};

/// What the device decides about its answers.
pub trait Device {
    /// Whether the device reports Shader Model 6 (`SM6Caps`).
    ///
    /// That is when `DXMT_D3D12_SM6` is not 0, or a DXIL capture folder is set.
    fn sm6_caps(&self) -> bool;

    /// Whether the adapter is advertised as NUMA (`advertise_numa_`).
    fn advertise_numa(&self) -> bool;

    /// Whether Metal supports textures of this sample count.
    fn supports_texture_sample_count(&self, count: u32) -> bool;
}

/// The `u32` at `offset` of a structure the caller has checked the buffer for.
fn get(data: &[u8], offset: usize) -> u32 {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// Stores a `u32` at `offset` of a structure the caller has checked the buffer for.
fn put(data: &mut [u8], offset: usize, value: u32) {
    if let Some(bytes) = data.get_mut(offset..offset + 4) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}

/// Stores a byte at `offset` of a structure the caller has checked the buffer for.
fn put_u8(data: &mut [u8], offset: usize, value: u8) {
    if let Some(byte) = data.get_mut(offset) {
        *byte = value;
    }
}

/// Stores the fields of a structure, each as the `u32` (or the 0 or 1 of a `BOOL`) given.
macro_rules! set {
    ($data:expr, $ty:ident { $($field:ident: $value:expr),* $(,)? }) => {
        $(put($data, offset_of!($ty, $field), $value);)*
    };
}

/// Refuses a buffer that is not the size of the structure, else gives the arm's `body`.
macro_rules! sized {
    ($data:expr, $ty:ty, $body:block) => {
        if $data.len() == size_of::<$ty>() $body else { E_INVALIDARG }
    };
}

/// Zeroes the whole buffer of a structure the device reports none of (`FEATURE_DATA`).
macro_rules! zeroed {
    ($data:expr, $ty:ty) => {
        sized!($data, $ty, {
            $data.fill(0);
            S_OK
        })
    };
}

/// `D3D_FEATURE_LEVEL` of the answer to `FEATURE_LEVELS`: the highest requested, capped.
///
/// # Safety
///
/// `data` is a `FeatureLevels` whose `feature_levels_requested` points at `num_feature_levels`
/// readable `u32`s, with no alignment promised, unless that is 0.
unsafe fn feature_levels(device: &impl Device, data: &mut [u8]) -> Hresult {
    let count = get(data, offset_of!(FeatureLevels, num_feature_levels));
    if count == 0 {
        return E_INVALIDARG;
    }
    let requested = data
        .get(offset_of!(FeatureLevels, feature_levels_requested)..)
        .and_then(|bytes| bytes.first_chunk::<{ size_of::<usize>() }>())
        .map_or(0, |bytes| usize::from_le_bytes(*bytes));
    if requested == 0 {
        return E_INVALIDARG;
    }
    let levels = core::ptr::with_exposed_provenance::<u32>(requested);
    // The C++ compares the enumerators as signed numbers: a level with the top bit set is below
    // every other.
    let mut highest = 0_i32;
    for index in 0..count {
        // SAFETY: the caller promises `count` readable `u32`s at `levels`, with no alignment.
        let level = unsafe { levels.wrapping_add(index as usize).read_unaligned() };
        highest = highest.max(level.cast_signed());
    }
    let cap = if device.sm6_caps() {
        D3D_FEATURE_LEVEL_12_1
    } else {
        D3D_FEATURE_LEVEL_11_1
    };
    put(
        data,
        offset_of!(FeatureLevels, max_supported_feature_level),
        highest.min(cap.cast_signed()).cast_unsigned(),
    );
    S_OK
}

/// What `MULTISAMPLE_QUALITY_LEVELS` says of a format and a sample count.
fn multisample_quality_levels(device: &impl Device, data: &mut [u8]) -> Hresult {
    let format = get(data, offset_of!(MultisampleQualityLevels, format));
    let count = get(data, offset_of!(MultisampleQualityLevels, sample_count));
    let flags = offset_of!(MultisampleQualityLevels, flags);
    let levels = offset_of!(MultisampleQualityLevels, num_quality_levels);
    if count == 0 {
        put(data, flags, 0);
        put(data, levels, 0);
        return E_FAIL;
    }
    if format == 0 {
        put(data, levels, u32::from(count == 1));
        return S_OK;
    }
    put(data, flags, 0);
    if query_dxgi(format).0 >= 0 {
        put(
            data,
            levels,
            u32::from(device.supports_texture_sample_count(count)),
        );
        S_OK
    } else {
        put(data, levels, 0);
        E_FAIL
    }
}

/// Fills the buffer for `feature` (`CheckFeatureSupport`).
///
/// # Safety
///
/// For `FEATURE_LEVELS`, the `u32` array `data` points at must be readable for its count (the
/// C++ reads it as it is); `data` itself is the application's buffer for the call and nothing
/// else is touched.
pub unsafe fn check_feature_support(
    device: &impl Device,
    feature: u32,
    data: &mut [u8],
) -> Hresult {
    let sm6 = u32::from(device.sm6_caps());
    match feature {
        D3D12_FEATURE_D3D12_OPTIONS22 => sized!(data, Options22, {
            set!(
                data,
                Options22 {
                    shader_execution_reordering_actually_reorders: 0,
                    create_byte_offset_views_supported: 0,
                    max1_d_dispatch_size: D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION,
                    max1_d_dispatch_mesh_size: D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_ARCHITECTURE => sized!(data, Architecture, {
            if get(data, offset_of!(Architecture, node_index)) > 0 {
                return E_INVALIDARG;
            }
            set!(
                data,
                Architecture {
                    cache_coherent_uma: 0,
                    tile_based_renderer: 1,
                    uma: u32::from(!device.advertise_numa()),
                }
            );
            S_OK
        }),
        D3D12_FEATURE_ARCHITECTURE1 => sized!(data, Architecture1, {
            if get(data, offset_of!(Architecture1, node_index)) > 0 {
                return E_INVALIDARG;
            }
            set!(
                data,
                Architecture1 {
                    cache_coherent_uma: 0,
                    tile_based_renderer: 1,
                    uma: u32::from(!device.advertise_numa()),
                    isolated_mmu: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS => sized!(data, MultisampleQualityLevels, {
            multisample_quality_levels(device, data)
        }),
        D3D12_FEATURE_ROOT_SIGNATURE => sized!(data, RootSignature, {
            let version = get(data, offset_of!(RootSignature, highest_version));
            if version == D3D_ROOT_SIGNATURE_VERSION_1_0
                || version == D3D_ROOT_SIGNATURE_VERSION_1_1
            {
                S_OK
            } else {
                E_INVALIDARG
            }
        }),
        D3D12_FEATURE_FEATURE_LEVELS => sized!(data, FeatureLevels, {
            // SAFETY: the caller's promise about the requested levels is this function's.
            unsafe { feature_levels(device, data) }
        }),
        D3D12_FEATURE_FORMAT_INFO => sized!(data, FormatInfo, {
            let format = get(data, offset_of!(FormatInfo, format));
            let plane_count = offset_of!(FormatInfo, plane_count);
            if format == 0 {
                put_u8(data, plane_count, 1);
                return S_OK;
            }
            let (hr, desc) = query_dxgi(format);
            if hr < 0 {
                return E_FAIL;
            }
            put_u8(
                data,
                plane_count,
                u8::try_from(desc.planar_count).unwrap_or(u8::MAX),
            );
            S_OK
        }),
        D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT => sized!(data, GpuVirtualAddressSupport, {
            set!(
                data,
                GpuVirtualAddressSupport {
                    max_gpu_virtual_address_bits_per_process: 48,
                    max_gpu_virtual_address_bits_per_resource: 48,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_SHADER_MODEL => sized!(data, ShaderModel, {
            let highest = offset_of!(ShaderModel, highest_shader_model);
            let requested = get(data, highest);
            put(
                data,
                highest,
                if sm6 != 0 {
                    // Signed, as the C++ compares: a model with the top bit set comes back.
                    requested
                        .cast_signed()
                        .min(D3D_SHADER_MODEL_6_8.cast_signed())
                        .cast_unsigned()
                } else {
                    D3D_SHADER_MODEL_5_1
                },
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS => sized!(data, Options, {
            set!(data, Options {
                double_precision_float_shader_ops: 0,
                output_merger_logic_op: 0,
                min_precision_support: D3D12_SHADER_MIN_PRECISION_SUPPORT_16_BIT,
                tiled_resources_tier: 0,
                resource_binding_tier: if sm6 != 0 {
                    D3D12_RESOURCE_BINDING_TIER_3
                } else {
                    D3D12_RESOURCE_BINDING_TIER_2
                },
                ps_specified_stencil_ref_supported: 1,
                typed_uav_load_additional_formats: 1,
                ro_vs_supported: 1,
                conservative_rasterization_tier: 0,
                max_gpu_virtual_address_bits_per_resource: 48,
                standard_swizzle64_kb_supported: 1,
                cross_node_sharing_tier: 0,
                cross_adapter_row_major_texture_supported: 0,
                vp_and_rt_array_index_from_any_shader_feeding_rasterizer_supported_without_gs_emulation: 1,
                resource_heap_tier: D3D12_RESOURCE_HEAP_TIER_2,
            });
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS16 => sized!(data, Options16, {
            set!(
                data,
                Options16 {
                    gpu_upload_heap_supported: 0,
                    dynamic_depth_bias_supported: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS2 => sized!(data, Options2, {
            set!(
                data,
                Options2 {
                    depth_bounds_test_supported: 0,
                    programmable_sample_positions_tier: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS3 => sized!(data, Options3, {
            set!(
                data,
                Options3 {
                    casting_fully_typed_format_supported: 1,
                    write_buffer_immediate_support_flags: 0,
                    view_instancing_tier: 0,
                    barycentrics_supported: 0,
                    copy_queue_timestamp_queries_supported: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS1 => sized!(data, Options1, {
            set!(
                data,
                Options1 {
                    wave_ops: sm6,
                    wave_lane_count_min: if sm6 != 0 { 32 } else { 0 },
                    wave_lane_count_max: if sm6 != 0 { 32 } else { 0 },
                    total_lane_count: 0,
                    expanded_compute_resource_states: 1,
                    int64_shader_ops: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS9 => sized!(data, Options9, {
            data.fill(0);
            set!(
                data,
                Options9 {
                    atomic_int64_on_typed_resource_supported: sm6,
                    atomic_int64_on_group_shared_supported: sm6,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS12 => sized!(data, Options12, {
            set!(
                data,
                Options12 {
                    relaxed_format_casting_supported: 0,
                    enhanced_barriers_supported: 1,
                    ms_primitives_pipeline_statistic_includes_culled_primitives: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS4 => sized!(data, Options4, {
            set!(
                data,
                Options4 {
                    msaa64_kb_aligned_texture_supported: 1,
                    shared_resource_compatibility_tier: 0,
                    native16_bit_shader_ops_supported: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS7 => sized!(data, Options7, {
            set!(
                data,
                Options7 {
                    mesh_shader_tier: 0,
                    sampler_feedback_tier: 0,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_SHADER_CACHE => sized!(data, ShaderCache, {
            set!(
                data,
                ShaderCache {
                    support_flags: D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_INPROC_CACHE
                        | D3D12_SHADER_CACHE_SUPPORT_AUTOMATIC_DISK_CACHE,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_FORMAT_SUPPORT => sized!(data, FormatSupport, {
            let unknown = get(data, offset_of!(FormatSupport, format)) == 0;
            set!(
                data,
                FormatSupport {
                    support1: if unknown {
                        D3D12_FORMAT_SUPPORT1_BUFFER
                    } else {
                        u32::MAX
                    },
                    support2: if unknown { 0 } else { u32::MAX },
                }
            );
            S_OK
        }),
        D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT => {
            sized!(data, ProtectedResourceSessionSupport, {
                if get(
                    data,
                    offset_of!(ProtectedResourceSessionSupport, node_index),
                ) != 0
                {
                    return E_INVALIDARG;
                }
                set!(data, ProtectedResourceSessionSupport { support: 0 });
                S_OK
            })
        }
        D3D12_FEATURE_EXISTING_HEAPS => zeroed!(data, ExistingHeaps),
        D3D12_FEATURE_CROSS_NODE | D3D12_FEATURE_DISPLAYABLE => E_INVALIDARG,
        D3D12_FEATURE_D3D12_OPTIONS5 => zeroed!(data, Options5),
        D3D12_FEATURE_D3D12_OPTIONS6 => zeroed!(data, Options6),
        D3D12_FEATURE_D3D12_OPTIONS8 => zeroed!(data, Options8),
        D3D12_FEATURE_D3D12_OPTIONS10 => zeroed!(data, Options10),
        D3D12_FEATURE_D3D12_OPTIONS11 => sized!(data, Options11, {
            data.fill(0);
            set!(
                data,
                Options11 {
                    atomic_int64_on_descriptor_heap_resource_supported: sm6,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS13 => zeroed!(data, Options13),
        D3D12_FEATURE_D3D12_OPTIONS14 => zeroed!(data, Options14),
        D3D12_FEATURE_D3D12_OPTIONS15 => zeroed!(data, Options15),
        D3D12_FEATURE_D3D12_OPTIONS17 => zeroed!(data, Options17),
        D3D12_FEATURE_D3D12_OPTIONS18 => zeroed!(data, Options18),
        D3D12_FEATURE_D3D12_OPTIONS19 => sized!(data, Options19, {
            data.fill(0);
            set!(
                data,
                Options19 {
                    max_sampler_descriptor_heap_size: 2048,
                    max_sampler_descriptor_heap_size_with_static_samplers: 2048,
                    max_view_descriptor_heap_size: 1_000_000,
                }
            );
            S_OK
        }),
        D3D12_FEATURE_D3D12_OPTIONS21 => sized!(data, Options21, {
            data.fill(0);
            set!(
                data,
                Options21 {
                    execute_indirect_tier: D3D12_EXECUTE_INDIRECT_TIER_1_0,
                    sample_cmp_gradient_and_bias_supported: sm6,
                    extended_command_info_supported: sm6,
                }
            );
            S_OK
        }),
        _ => E_NOTIMPL,
    }
}

#[cfg(test)]
mod tests;
