//! NVIDIA's NVAPI as DXMT answers it: statuses, the structs the functions write, versions.
//!
//! Sizes and offsets are those of `external/nvapi`'s headers under llvm-mingw, identical for
//! x86-64 and ARM64EC; each struct asserts its size, so a wrong mirror fails the build.

/// `NVAPI_SDK_VERSION`, which the driver-version strings carry.
pub const NVAPI_SDK_VERSION: u32 = 57218;

/// `NVAPI_MAX_PHYSICAL_GPUS` and `NVAPI_MAX_LOGICAL_GPUS`.
pub const MAX_GPUS: usize = 64;

/// `NVAPI_SHORT_STRING_MAX`.
pub const SHORT_STRING_MAX: usize = 64;

/// `NvAPI_ShortString`.
pub type ShortString = [u8; SHORT_STRING_MAX];

/// `NvAPI_Status`: `OK`, or the error a function answers with.
pub type Status = i32;

pub const OK: Status = 0;
pub const NO_IMPLEMENTATION: Status = -3;
pub const API_NOT_INITIALIZED: Status = -4;
pub const INVALID_ARGUMENT: Status = -5;
pub const NVIDIA_DEVICE_NOT_FOUND: Status = -6;
pub const END_ENUMERATION: Status = -7;
pub const INCOMPATIBLE_STRUCT_VERSION: Status = -9;
pub const NOT_SUPPORTED: Status = -104;
pub const STEREO_NOT_INITIALIZED: Status = -140;
pub const INSUFFICIENT_BUFFER: Status = -174;
pub const MOSAIC_NOT_ACTIVE: Status = -176;

/// `MAKE_NVAPI_VERSION`: a struct's size, with its version in the high half.
#[must_use]
pub const fn version(size: usize, v: u32) -> u32 {
    let bytes = size.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) | (v << 16)
}

/// `NV_DISPLAY_DRIVER_VERSION`.
#[repr(C)]
pub struct DisplayDriverVersion {
    pub version: u32,
    pub drv_version: u32,
    pub bld_change_list_num: u32,
    pub build_branch_string: ShortString,
    pub adapter_string: ShortString,
}
const _: () = assert!(size_of::<DisplayDriverVersion>() == 140);
pub const DISPLAY_DRIVER_VERSION_VER: u32 = version(size_of::<DisplayDriverVersion>(), 1);

/// `NV_GET_CURRENT_SLI_STATE_V1`.
#[repr(C)]
pub struct SliStateV1 {
    pub version: u32,
    pub max_num_afr_groups: u32,
    pub num_afr_groups: u32,
    pub current_afr_index: u32,
    pub next_frame_afr_index: u32,
    pub previous_frame_afr_index: u32,
    pub is_cur_afr_group_new: u32,
}
const _: () = assert!(size_of::<SliStateV1>() == 28);
pub const SLI_STATE_VER1: u32 = version(size_of::<SliStateV1>(), 1);

/// `NV_GET_CURRENT_SLI_STATE_V2`, whose version number is 1 as V1's.
#[repr(C)]
pub struct SliStateV2 {
    pub v1: SliStateV1,
    pub num_vr_sli_gpus: u32,
}
const _: () = assert!(size_of::<SliStateV2>() == 32);
pub const SLI_STATE_VER2: u32 = version(size_of::<SliStateV2>(), 1);

/// The static metadata descriptor of `NV_HDR_CAPABILITIES`'s `display_data`.
///
/// Type 1, SMPTE ST 2086.
#[repr(C)]
pub struct HdrDisplayData {
    pub primary_x0: u16,
    pub primary_y0: u16,
    pub primary_x1: u16,
    pub primary_y1: u16,
    pub primary_x2: u16,
    pub primary_y2: u16,
    pub white_point_x: u16,
    pub white_point_y: u16,
    pub desired_content_max_luminance: u16,
    pub desired_content_min_luminance: u16,
    pub desired_content_max_frame_average_luminance: u16,
}

/// Bit 0 of the capabilities' flags: `isST2084EotfSupported`.
pub const HDR_CAP_ST2084_EOTF: u32 = 1;

/// `NV_HDR_CAPABILITIES_V1`: V2 and V3 start with it.
#[repr(C)]
pub struct HdrCapabilitiesV1 {
    pub version: u32,
    pub flags: u32,
    pub static_metadata_descriptor_id: u32,
    pub display_data: HdrDisplayData,
}
const _: () = assert!(size_of::<HdrCapabilitiesV1>() == 36);
pub const HDR_CAPABILITIES_VER1: u32 = version(size_of::<HdrCapabilitiesV1>(), 1);

/// The Dolby Vision metadata after `display_data` in V2 and V3.
///
/// A flags word, then ten chromaticity and luminance values.
#[repr(C)]
pub struct DvStaticMetadata {
    pub flags: u32,
    pub target_min_luminance: u16,
    pub target_max_luminance: u16,
    pub cc_red_x: u16,
    pub cc_red_y: u16,
    pub cc_green_x: u16,
    pub cc_green_y: u16,
    pub cc_blue_x: u16,
    pub cc_blue_y: u16,
    pub cc_white_x: u16,
    pub cc_white_y: u16,
}

/// `NV_HDR_CAPABILITIES_V2`.
#[repr(C)]
pub struct HdrCapabilitiesV2 {
    pub v1: HdrCapabilitiesV1,
    pub dv_static_metadata: DvStaticMetadata,
}
const _: () = assert!(size_of::<HdrCapabilitiesV2>() == 60);
pub const HDR_CAPABILITIES_VER2: u32 = version(size_of::<HdrCapabilitiesV2>(), 2);

/// `NV_HDR_CAPABILITIES_V3`, the current `NV_HDR_CAPABILITIES`.
#[repr(C)]
pub struct HdrCapabilitiesV3 {
    pub v2: HdrCapabilitiesV2,
    pub hdr10plus_vsvdb: u16,
}
const _: () = assert!(size_of::<HdrCapabilitiesV3>() == 64);
pub const HDR_CAPABILITIES_VER3: u32 = version(size_of::<HdrCapabilitiesV3>(), 3);

/// `NV_HDR_COLOR_DATA`'s `mastering_display_data`.
#[repr(C)]
pub struct HdrMasteringData {
    pub primary_x0: u16,
    pub primary_y0: u16,
    pub primary_x1: u16,
    pub primary_y1: u16,
    pub primary_x2: u16,
    pub primary_y2: u16,
    pub white_point_x: u16,
    pub white_point_y: u16,
    pub max_display_mastering_luminance: u16,
    pub min_display_mastering_luminance: u16,
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}

/// `NV_HDR_COLOR_DATA_V1`.
#[repr(C)]
pub struct HdrColorDataV1 {
    pub version: u32,
    pub cmd: u32,
    pub hdr_mode: u32,
    pub static_metadata_descriptor_id: u32,
    pub mastering_display_data: HdrMasteringData,
}
const _: () = assert!(size_of::<HdrColorDataV1>() == 40);
pub const HDR_COLOR_DATA_VER1: u32 = version(size_of::<HdrColorDataV1>(), 1);

/// `NV_HDR_COLOR_DATA_V2`, the current `NV_HDR_COLOR_DATA`.
#[repr(C)]
pub struct HdrColorDataV2 {
    pub v1: HdrColorDataV1,
    pub hdr_color_format: u32,
    pub hdr_dynamic_range: u32,
    pub hdr_bpc: u32,
}
const _: () = assert!(size_of::<HdrColorDataV2>() == 52);
pub const HDR_COLOR_DATA_VER2: u32 = version(size_of::<HdrColorDataV2>(), 2);

/// `NV_HDR_CMD_GET`.
pub const HDR_CMD_GET: u32 = 0;
/// `NV_HDR_MODE_OFF`, `NV_HDR_MODE_UHDA` (scRGB) and `NV_HDR_MODE_UHDA_PASSTHROUGH` (PQ).
pub const HDR_MODE_OFF: u32 = 0;
pub const HDR_MODE_UHDA: u32 = 2;
pub const HDR_MODE_UHDA_PASSTHROUGH: u32 = 5;
/// `NV_COLOR_FORMAT_RGB`, `NV_DYNAMIC_RANGE_VESA` and `NV_BPC_10`.
pub const COLOR_FORMAT_RGB: u32 = 0;
pub const DYNAMIC_RANGE_VESA: u32 = 0;
pub const BPC_10: u32 = 3;

/// `NV_GPU_DISPLAYIDS`: after `display_id`, one word of bit fields.
#[repr(C)]
pub struct GpuDisplayIds {
    pub version: u32,
    pub connector_type: i32,
    pub display_id: u32,
    pub flags: u32,
}
const _: () = assert!(size_of::<GpuDisplayIds>() == 16);
pub const GPU_DISPLAYIDS_VER1: u32 = version(size_of::<GpuDisplayIds>(), 1);
/// `NV_GPU_DISPLAYIDS_VER2`: version number 3.
pub const GPU_DISPLAYIDS_VER2: u32 = version(size_of::<GpuDisplayIds>(), 3);
/// `isActive`, `isConnected` and `isPhysicallyConnected`, as bits of `flags`.
pub const DISPLAYIDS_ACTIVE: u32 = 1 << 2;
pub const DISPLAYIDS_CONNECTED: u32 = 1 << 6;
pub const DISPLAYIDS_PHYSICALLY_CONNECTED: u32 = 1 << 17;
/// `NV_MONITOR_CONN_TYPE_UNKNOWN`.
pub const MONITOR_CONN_TYPE_UNKNOWN: i32 = -1;

/// `NV_GPU_ARCH_INFO_V1` and `_V2`: the same three words (V2 names them `*_id` too).
#[repr(C)]
pub struct GpuArchInfo {
    pub version: u32,
    pub architecture: u32,
    pub implementation: u32,
    pub revision: u32,
}
const _: () = assert!(size_of::<GpuArchInfo>() == 16);
pub const GPU_ARCH_INFO_VER1: u32 = version(size_of::<GpuArchInfo>(), 1);
pub const GPU_ARCH_INFO_VER2: u32 = version(size_of::<GpuArchInfo>(), 2);
/// `NV_GPU_ARCHITECTURE_AD100`, `NV_GPU_ARCH_IMPLEMENTATION_AD102` and `NV_GPU_CHIP_REV_UNKNOWN`.
pub const GPU_ARCHITECTURE_AD100: u32 = 0x190;
pub const GPU_ARCH_IMPLEMENTATION_AD102: u32 = 2;
pub const GPU_CHIP_REV_UNKNOWN: u32 = 0xffff_ffff;

/// `NV_GET_SLEEP_STATUS_PARAMS_V1`.
#[repr(C)]
pub struct GetSleepStatusParams {
    pub version: u32,
    pub low_latency_mode: u8,
    pub fs_vrr: u8,
    pub cpl_vsync_on: u8,
    pub sleep_interval_us: u32,
    pub use_game_sleep: u8,
    pub rsvd: [u8; 121],
}
const _: () = assert!(size_of::<GetSleepStatusParams>() == 136);
pub const GET_SLEEP_STATUS_PARAMS_VER1: u32 = version(size_of::<GetSleepStatusParams>(), 1);

/// `NV_SET_SLEEP_MODE_PARAMS_V1`: the function reads only its version.
#[repr(C)]
pub struct SetSleepModeParams {
    pub version: u32,
    pub low_latency_mode: u8,
    pub low_latency_boost: u8,
    pub minimum_interval_us: u32,
    pub use_markers_to_optimize: u8,
    pub rsvd: [u8; 31],
}
const _: () = assert!(size_of::<SetSleepModeParams>() == 44);
pub const SET_SLEEP_MODE_PARAMS_VER1: u32 = version(size_of::<SetSleepModeParams>(), 1);

/// `NV_LATENCY_MARKER_PARAMS_V1`: the function reads only its version.
#[repr(C)]
pub struct LatencyMarkerParams {
    pub version: u32,
    pub frame_id: u64,
    pub marker_type: u32,
    pub rsvd0: u64,
    pub rsvd: [u8; 56],
}
const _: () = assert!(size_of::<LatencyMarkerParams>() == 88);
pub const LATENCY_MARKER_PARAMS_VER1: u32 = version(size_of::<LatencyMarkerParams>(), 1);

/// One of `NV_LATENCY_RESULT_PARAMS`' 64 frame reports.
///
/// Fourteen timestamps, two durations, reserved bytes.
#[repr(C)]
pub struct FrameReport {
    pub times: [u64; 14],
    pub gpu_active_render_time_us: u32,
    pub gpu_frame_time_us: u32,
    pub rsvd: [u8; 120],
}
const _: () = assert!(size_of::<FrameReport>() == 240);

/// `NV_LATENCY_RESULT_PARAMS_V1`.
#[repr(C)]
pub struct LatencyResultParams {
    pub version: u32,
    pub frame_report: [FrameReport; 64],
    pub rsvd: [u8; 32],
}
const _: () = assert!(size_of::<LatencyResultParams>() == 15400);
pub const LATENCY_RESULT_PARAMS_VER1: u32 = version(size_of::<LatencyResultParams>(), 1);

/// `NV_LOGICAL_GPU_DATA_V1`.
#[repr(C)]
pub struct LogicalGpuData {
    pub version: u32,
    pub os_adapter_id: *mut core::ffi::c_void,
    pub physical_gpu_count: u32,
    pub physical_gpu_handles: [*mut core::ffi::c_void; MAX_GPUS],
    pub reserved: [u32; 8],
}
const _: () = assert!(size_of::<LogicalGpuData>() == 568);
pub const LOGICAL_GPU_DATA_VER1: u32 = version(size_of::<LogicalGpuData>(), 1);

/// The shader extension opcodes `NvAPI_D3D11_IsNvShaderExtnOpCodeSupported` knows by name.
pub const EXTN_OP_SHFL: u32 = 1;
pub const EXTN_OP_FP16_ATOMIC: u32 = 12;
pub const EXTN_OP_UINT64_ATOMIC: u32 = 20;

// The offsets the functions' writes depend on, from the headers' own layout.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(DisplayDriverVersion, build_branch_string) == 12);
    assert!(offset_of!(DisplayDriverVersion, adapter_string) == 76);
    assert!(offset_of!(HdrCapabilitiesV1, display_data) == 12);
    assert!(offset_of!(HdrCapabilitiesV2, dv_static_metadata) == 36);
    assert!(offset_of!(HdrCapabilitiesV3, hdr10plus_vsvdb) == 60);
    assert!(offset_of!(HdrColorDataV1, mastering_display_data) == 16);
    assert!(offset_of!(HdrColorDataV2, hdr_color_format) == 40);
    assert!(offset_of!(GpuDisplayIds, flags) == 12);
    assert!(offset_of!(GetSleepStatusParams, sleep_interval_us) == 8);
    assert!(offset_of!(GetSleepStatusParams, use_game_sleep) == 12);
    assert!(offset_of!(LatencyResultParams, frame_report) == 8);
    assert!(offset_of!(LatencyResultParams, rsvd) == 15368);
    assert!(offset_of!(LogicalGpuData, os_adapter_id) == 8);
    assert!(offset_of!(LogicalGpuData, physical_gpu_count) == 16);
    assert!(offset_of!(LogicalGpuData, physical_gpu_handles) == 24);
};
