//! NGX's results, features, flags and structures, as DXMT's `nvngx.hpp` has them.

/// `NVSDK_NGX_Result_Success`.
pub const OK: u32 = 1;
/// `NVSDK_NGX_Result_Fail`: what every failure's bits include.
pub const FAIL: u32 = 0xbad0_0000;
/// `NVSDK_NGX_Result_FAIL_FeatureNotSupported`.
pub const FEATURE_NOT_SUPPORTED: u32 = 0xbad0_0001;
/// `NVSDK_NGX_Result_FAIL_InvalidParameter`.
pub const INVALID_PARAMETER: u32 = 0xbad0_0005;

/// Whether `result` is a failure, as `NVSDK_NGX_FAILED` tests it.
pub const fn failed(result: u32) -> bool {
    result & FAIL == FAIL
}

/// `NVSDK_NGX_Feature_SuperSampling`: DLSS super resolution, the one feature nvngx makes.
pub const FEATURE_SUPERSAMPLING: u32 = 1;

/// `NVSDK_NGX_DLSS_Feature_Flags`: motion vectors at the render resolution.
pub const FLAG_MV_LOWRES: u32 = 1 << 1;
/// Motion vectors that include the jitter.
pub const FLAG_MV_JITTERED: u32 = 1 << 2;
/// A depth buffer with 1 nearest.
pub const FLAG_DEPTH_INVERTED: u32 = 1 << 3;
/// No exposure texture: MetalFX works the exposure out.
pub const FLAG_AUTO_EXPOSURE: u32 = 1 << 6;

/// `NVSDK_NGX_FeatureDiscoveryInfo`.
#[repr(C)]
pub struct FeatureDiscoveryInfo {
    pub sdk_version: u32,
    pub feature_id: u32,
    pub identifier: [u8; 32],
    pub application_data_path: *const u16,
    pub feature_info: *const core::ffi::c_void,
}

/// `NVSDK_NGX_FeatureRequirement`.
#[repr(C)]
pub struct FeatureRequirement {
    pub feature_supported: u32,
    pub min_hw_architecture: u32,
    pub min_os_version: [u8; 255],
}

/// `NVSDK_NGX_FeatureSupportResult_Supported`.
pub const SUPPORTED: u32 = 0;
/// `NVSDK_NGX_FeatureSupportResult_NotImplemented`, as DXMT answers any other feature.
pub const UNSUPPORTED: u32 = 1;

const _: () = {
    use core::mem::offset_of;
    assert!(size_of::<FeatureDiscoveryInfo>() == 56 && offset_of!(FeatureDiscoveryInfo, application_data_path) == 40);
    assert!(size_of::<FeatureRequirement>() == 264 && offset_of!(FeatureRequirement, min_os_version) == 8);
};
