//! NGX's results, features, flags and structures, as DXMT's `nvngx.hpp` has them.

/// `NVSDK_NGX_Result_Success`.
pub const OK: u32 = 1;
/// `NVSDK_NGX_Result_Fail`: what every failure's bits include.
pub const FAIL: u32 = 0xbad0_0000;
/// `NVSDK_NGX_Result_FAIL_FeatureNotSupported`.
pub const FEATURE_NOT_SUPPORTED: u32 = 0xbad0_0001;
/// `NVSDK_NGX_Result_FAIL_InvalidParameter`.
pub const INVALID_PARAMETER: u32 = 0xbad0_0005;

/// `NVSDK_NGX_Feature_SuperSampling`: DLSS super resolution.
pub const FEATURE_SUPERSAMPLING: u32 = 1;
/// `NVSDK_NGX_Feature_FrameGeneration`: DLSS-G frame generation.
pub const FEATURE_FRAME_GENERATION: u32 = 11;
/// `NVSDK_NGX_Feature_RayReconstruction`: DLSS-D ray reconstruction.
pub const FEATURE_RAY_RECONSTRUCTION: u32 = 13;

/// `NVSDK_NGX_DLSS_Feature_Flags`: motion vectors at the render resolution.
pub const FLAG_MV_LOWRES: u32 = 1 << 1;
/// Motion vectors that include the jitter.
pub const FLAG_MV_JITTERED: u32 = 1 << 2;
/// A depth buffer with 1 nearest.
pub const FLAG_DEPTH_INVERTED: u32 = 1 << 3;
/// No exposure texture: `MetalFX` works the exposure out.
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

/// `NVSDK_NGX_Parameter_DLSS_Exposure_Scale`.
pub const PARAM_EXPOSURE_SCALE: &core::ffi::CStr = c"DLSS.Exposure.Scale";
/// `NVSDK_NGX_Parameter_DLSS_Input_Bias_Current_Color_Active`.
pub const PARAM_REACTIVE_ACTIVE: &core::ffi::CStr = c"DLSS.Input.Bias.Current.Color.Active";
/// `NVSDK_NGX_Parameter_DLSS_Input_Bias_Current_Color_Mask`.
pub const PARAM_REACTIVE_MASK: &core::ffi::CStr = c"DLSS.Input.Bias.Current.Color.Mask";
/// `NVSDK_NGX_Parameter_DLSS_Input_Bias_Current_Color_SubrectBase_X`.
pub const PARAM_REACTIVE_BASE_X: &core::ffi::CStr = c"DLSS.Input.Bias.Current.Color.SubrectBase.X";
/// `NVSDK_NGX_Parameter_DLSS_Input_Bias_Current_Color_SubrectBase_Y`.
pub const PARAM_REACTIVE_BASE_Y: &core::ffi::CStr = c"DLSS.Input.Bias.Current.Color.SubrectBase.Y";
/// `NVSDK_NGX_Parameter_DLSSG_CameraNear`.
pub const PARAM_DLSSG_CAMERA_NEAR: &core::ffi::CStr = c"DLSSG.CameraNear";
/// `NVSDK_NGX_Parameter_DLSSG_CameraFar`.
pub const PARAM_DLSSG_CAMERA_FAR: &core::ffi::CStr = c"DLSSG.CameraFar";
/// `NVSDK_NGX_Parameter_DLSSG_CameraFOV`.
pub const PARAM_DLSSG_CAMERA_FOV: &core::ffi::CStr = c"DLSSG.CameraFOV";
/// `NVSDK_NGX_Parameter_DLSSG_CameraAspectRatio`.
pub const PARAM_DLSSG_CAMERA_ASPECT: &core::ffi::CStr = c"DLSSG.CameraAspectRatio";

/// `NVSDK_NGX_Parameter_SuperSamplingDenoising_Available`.
pub const PARAM_DLSSD_AVAILABLE: &core::ffi::CStr = c"SuperSamplingDenoising.Available";
/// `NVSDK_NGX_Parameter_SuperSamplingDenoising_FeatureInitResult`.
pub const PARAM_DLSSD_FEATURE_INIT_RESULT: &core::ffi::CStr =
    c"SuperSamplingDenoising.FeatureInitResult";
/// `NVSDK_NGX_Parameter_SuperSamplingDenoising_NeedsUpdatedDriver`.
pub const PARAM_DLSSD_NEEDS_UPDATED_DRIVER: &core::ffi::CStr =
    c"SuperSamplingDenoising.NeedsUpdatedDriver";
/// `NVSDK_NGX_Parameter_SuperSamplingDenoising_MinDriverVersionMajor`.
pub const PARAM_DLSSD_MIN_DRIVER_MAJOR: &core::ffi::CStr =
    c"SuperSamplingDenoising.MinDriverVersionMajor";
/// `NVSDK_NGX_Parameter_SuperSamplingDenoising_MinDriverVersionMinor`.
pub const PARAM_DLSSD_MIN_DRIVER_MINOR: &core::ffi::CStr =
    c"SuperSamplingDenoising.MinDriverVersionMinor";

/// `NVSDK_NGX_Parameter_DiffuseAlbedo`.
pub const PARAM_DIFFUSE_ALBEDO: &core::ffi::CStr = c"DiffuseAlbedo";
/// `NVSDK_NGX_Parameter_SpecularAlbedo`.
pub const PARAM_SPECULAR_ALBEDO: &core::ffi::CStr = c"SpecularAlbedo";
/// `NVSDK_NGX_Parameter_Normals`.
pub const PARAM_NORMALS: &core::ffi::CStr = c"Normals";
/// `NVSDK_NGX_Parameter_Roughness`.
pub const PARAM_ROUGHNESS: &core::ffi::CStr = c"Roughness";
/// `NVSDK_NGX_Parameter_SpecularHitDistance`.
pub const PARAM_SPECULAR_HIT_DISTANCE: &core::ffi::CStr = c"SpecularHitDistance";
/// `NVSDK_NGX_Parameter_WorldToViewMatrix`.
pub const PARAM_WORLD_TO_VIEW_MATRIX: &core::ffi::CStr = c"WorldToViewMatrix";
/// `NVSDK_NGX_Parameter_ViewToClipMatrix`.
pub const PARAM_VIEW_TO_CLIP_MATRIX: &core::ffi::CStr = c"ViewToClipMatrix";
/// `DenoiseStrengthMask`.
pub const PARAM_DENOISE_STRENGTH_MASK: &core::ffi::CStr = c"DenoiseStrengthMask";
/// `TransparencyOverlay`.
pub const PARAM_TRANSPARENCY_OVERLAY: &core::ffi::CStr = c"TransparencyOverlay";

/// `DLSSD.DiffuseAlbedo`.
pub const PARAM_DLSSD_DIFFUSE_ALBEDO: &core::ffi::CStr = c"DLSSD.DiffuseAlbedo";
/// `DLSSD.SpecularAlbedo`.
pub const PARAM_DLSSD_SPECULAR_ALBEDO: &core::ffi::CStr = c"DLSSD.SpecularAlbedo";
/// `DLSSD.Normals`.
pub const PARAM_DLSSD_NORMALS: &core::ffi::CStr = c"DLSSD.Normals";
/// `DLSSD.Roughness`.
pub const PARAM_DLSSD_ROUGHNESS: &core::ffi::CStr = c"DLSSD.Roughness";
/// `DLSSD.SpecularHitDistance`.
pub const PARAM_DLSSD_SPECULAR_HIT_DISTANCE: &core::ffi::CStr = c"DLSSD.SpecularHitDistance";
/// `DLSSD.WorldToViewMatrix`.
pub const PARAM_DLSSD_WORLD_TO_VIEW_MATRIX: &core::ffi::CStr = c"DLSSD.WorldToViewMatrix";
/// `DLSSD.ViewToClipMatrix`.
pub const PARAM_DLSSD_VIEW_TO_CLIP_MATRIX: &core::ffi::CStr = c"DLSSD.ViewToClipMatrix";
/// `DLSSD.DenoiseStrengthMask`.
pub const PARAM_DLSSD_DENOISE_STRENGTH_MASK: &core::ffi::CStr = c"DLSSD.DenoiseStrengthMask";
/// `DLSSD.TransparencyOverlay`.
pub const PARAM_DLSSD_TRANSPARENCY_OVERLAY: &core::ffi::CStr = c"DLSSD.TransparencyOverlay";

/// `NVSDK_NGX_Parameter_FrameGeneration_Available`.
pub const PARAM_DLSSG_AVAILABLE: &core::ffi::CStr = c"FrameGeneration.Available";
/// `NVSDK_NGX_Parameter_FrameGeneration_FeatureInitResult`.
pub const PARAM_DLSSG_FEATURE_INIT_RESULT: &core::ffi::CStr = c"FrameGeneration.FeatureInitResult";
/// `NVSDK_NGX_Parameter_FrameGeneration_NeedsUpdatedDriver`.
pub const PARAM_DLSSG_NEEDS_UPDATED_DRIVER: &core::ffi::CStr =
    c"FrameGeneration.NeedsUpdatedDriver";
/// `NVSDK_NGX_Parameter_FrameGeneration_MinDriverVersionMajor`.
pub const PARAM_DLSSG_MIN_DRIVER_MAJOR: &core::ffi::CStr = c"FrameGeneration.MinDriverVersionMajor";
/// `NVSDK_NGX_Parameter_FrameGeneration_MinDriverVersionMinor`.
pub const PARAM_DLSSG_MIN_DRIVER_MINOR: &core::ffi::CStr = c"FrameGeneration.MinDriverVersionMinor";

/// `DLSSG.HUDLess`.
pub const PARAM_DLSSG_HUDLESS: &core::ffi::CStr = c"DLSSG.HUDLess";
/// `DLSSG.UI`.
pub const PARAM_DLSSG_UI: &core::ffi::CStr = c"DLSSG.UI";
/// `DLSSG.UIColorAndAlpha`.
pub const PARAM_DLSSG_UI_COLOR_AND_ALPHA: &core::ffi::CStr = c"DLSSG.UIColorAndAlpha";
/// `DLSSG.UIComposited`.
pub const PARAM_DLSSG_UI_COMPOSITED: &core::ffi::CStr = c"DLSSG.UIComposited";
/// `DLSSG.MvecScaleX`.
pub const PARAM_DLSSG_MVEC_SCALE_X: &core::ffi::CStr = c"DLSSG.MvecScaleX";
/// `DLSSG.MvecScaleY`.
pub const PARAM_DLSSG_MVEC_SCALE_Y: &core::ffi::CStr = c"DLSSG.MvecScaleY";
/// `DLSSG.JitterOffsetX`.
pub const PARAM_DLSSG_JITTER_OFFSET_X: &core::ffi::CStr = c"DLSSG.JitterOffsetX";
/// `DLSSG.JitterOffsetY`.
pub const PARAM_DLSSG_JITTER_OFFSET_Y: &core::ffi::CStr = c"DLSSG.JitterOffsetY";
/// `DLSSG.WorldToViewMatrix`.
pub const PARAM_DLSSG_WORLD_TO_VIEW_MATRIX: &core::ffi::CStr = c"DLSSG.WorldToViewMatrix";
/// `DLSSG.ViewToClipMatrix`.
pub const PARAM_DLSSG_VIEW_TO_CLIP_MATRIX: &core::ffi::CStr = c"DLSSG.ViewToClipMatrix";
/// `DLSSG.DepthSubrectBase.X`.
pub const PARAM_DLSSG_DEPTH_SUBRECT_BASE_X: &core::ffi::CStr = c"DLSSG.DepthSubrectBase.X";
/// `DLSSG.DepthSubrectBase.Y`.
pub const PARAM_DLSSG_DEPTH_SUBRECT_BASE_Y: &core::ffi::CStr = c"DLSSG.DepthSubrectBase.Y";
/// `DLSSG.MvecSubrectBase.X`.
pub const PARAM_DLSSG_MVEC_SUBRECT_BASE_X: &core::ffi::CStr = c"DLSSG.MvecSubrectBase.X";
/// `DLSSG.MvecSubrectBase.Y`.
pub const PARAM_DLSSG_MVEC_SUBRECT_BASE_Y: &core::ffi::CStr = c"DLSSG.MvecSubrectBase.Y";
/// `DLSSG.BackbufferSubrectBase.X`.
pub const PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_X: &core::ffi::CStr =
    c"DLSSG.BackbufferSubrectBase.X";
/// `DLSSG.BackbufferSubrectBase.Y`.
pub const PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_Y: &core::ffi::CStr =
    c"DLSSG.BackbufferSubrectBase.Y";
/// `DLSSG.ContentWidth`.
pub const PARAM_DLSSG_CONTENT_WIDTH: &core::ffi::CStr = c"DLSSG.ContentWidth";
/// `DLSSG.ContentHeight`.
pub const PARAM_DLSSG_CONTENT_HEIGHT: &core::ffi::CStr = c"DLSSG.ContentHeight";

/// Private `ID3D12Object` data GUID carrying `DLSSG.Camera*` parameters to `d3d12`.
pub const DLSSG_CAMERA_GUID: dxmt_util::com::Guid = dxmt_util::com::Guid::new(
    0x7a3f_91c4,
    0x5e82,
    0x4b1d,
    [0x93, 0xa7, 0x2c, 0x41, 0x8f, 0xd0, 0x62, 0x19],
);

/// Camera parameters forwarded from `DLSSG.Camera*` NGX keys to `MetalFX`'s interpolator.
///
/// Zero fields mean "absent; use the interpolator default".
#[repr(C)]
pub struct DlssgCameraParams {
    pub near_plane: f32,
    pub far_plane: f32,
    pub field_of_view: f32,
    pub aspect_ratio: f32,
}

const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<FeatureDiscoveryInfo>() == 56
            && offset_of!(FeatureDiscoveryInfo, application_data_path) == 40
    );
    assert!(
        size_of::<FeatureRequirement>() == 264
            && offset_of!(FeatureRequirement, min_os_version) == 8
    );
    assert!(size_of::<DlssgCameraParams>() == 16);
};

/// A feature's head, which both APIs' features start with: the game's handle is its address.
#[repr(C)]
#[derive(Default)]
pub struct Dlss {
    pub handle: u32,
    pub feature: u32,
    /// 11 or 12: the API whose `CreateFeature` made it.
    pub api: u32,
    pub width: u32,
    pub height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub quality: i32,
    pub flag: i32,
    pub enable_output_subrects: i32,
}

/// `printf`'s `%#x`: `0`, or `0x` and lowercase hex digits.
pub fn hex(v: u32) -> String {
    if v == 0 {
        "0".to_owned()
    } else {
        format!("{v:#x}")
    }
}

/// `printf`'s `%p` in Windows' C runtime: 16 uppercase hex digits.
pub fn pointer(p: *const core::ffi::c_void) -> String {
    format!("{:016X}", p.addr())
}

/// Writes `text` to `out` as one line of the game's log: `macneutron-dlss: ` first.
///
/// A failed write is ignored, as C's `fprintf` ignores it: the game carries on whatever its
/// stderr is.
pub fn line(out: &mut impl std::io::Write, text: std::fmt::Arguments) {
    let _ = writeln!(out, "macneutron-dlss: {text}");
}

/// Writes `text` to stderr, the game's log, as `line` does.
pub fn note(text: std::fmt::Arguments) {
    line(&mut std::io::stderr(), text);
}

#[cfg(test)]
mod tests;
