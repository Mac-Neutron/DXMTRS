//! `MetalFX`: the temporal, temporal denoised and spatial scalers and the frame interpolator.
//!
//! Slots 56 to 59, 68, 69, 150 to 152 and 165 to 167. A guest's handle that is not what its slot
//! expects makes `MetalFX` raise when the first selector reaches it, and the exception leaves the
//! handler for the caller. A null handle is nil, which every send answers by doing nothing; a nil
//! device or command buffer is sent on as nil, whatever `MetalFX` makes of it. The four descriptor
//! classes and the scaler protocols are re-declared below, so that those nils can be sent and the
//! macOS 26/27 selectors the bindings lack can be.

use core::ffi::c_void;

use objc2::{
    AnyThread, extern_class, extern_methods,
    rc::{Allocated, Retained},
    runtime::{AnyObject, NSObject, ProtocolObject, Sel},
};
use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLFence, MTLPixelFormat, MTLTexture};
use objc2_metal_fx::{
    MTLFXFrameInterpolatableScaler, MTLFXFrameInterpolator, MTLFXSpatialScaler, MTLFXTemporalScaler,
};
use winemetal_wire::{
    UnixcallGenericObjUint64Ret, UnixcallMtlcommandbufferFrameInterpolate,
    UnixcallMtlcommandbufferSpatialScale, UnixcallMtlcommandbufferTemporalDenoisedScale,
    UnixcallMtlcommandbufferTemporalScale, UnixcallMtldeviceFxtemporalscalerScale,
    UnixcallMtldeviceNewfxframeinterpolator, UnixcallMtldeviceNewfxspatialscaler,
    UnixcallMtldeviceNewfxtemporaldenoisedscaler, UnixcallMtldeviceNewfxtemporalscaler,
    WMTFXFrameInterpolatorInfo, WMTFXFrameInterpolatorProps, WMTFXSpatialScalerInfo,
    WMTFXTemporalDenoisedScalerInfo, WMTFXTemporalDenoisedScalerProps, WMTFXTemporalScalerInfo,
    WMTFXTemporalScalerProps,
};

use crate::{
    ffi::{self, AbortOnPanic, STATUS_SUCCESS},
    format::pixel_format,
    handle,
};

/// Sends a 4x4 float matrix setter (`simd_float4x4`) via `objc_msgSend`.
///
/// On ARM64 macOS, Clang passes `simd_float4x4` as a 4-register HVA in `q0..q3`, whereas Rust
/// passes a 64-byte `#[repr(C)]` struct indirectly via `x0`.
///
/// # Safety
///
/// `obj` must point to a valid Objective-C receiver implementing `sel` taking `simd_float4x4`.
#[cfg(target_arch = "aarch64")]
unsafe fn send_set_float4x4(obj: *const AnyObject, sel: Sel, m: &[f32; 16]) {
    use core::arch::aarch64::{float32x4_t, vld1q_f32};

    let p0 = m.as_ptr();
    // SAFETY: `m` has 16 floats, so 4 floats are readable from index 0.
    let c0 = unsafe { vld1q_f32(p0) };
    let p1 = m[4..].as_ptr();
    // SAFETY: `m` has 16 floats, so 4 floats are readable from index 4.
    let c1 = unsafe { vld1q_f32(p1) };
    let p2 = m[8..].as_ptr();
    // SAFETY: `m` has 16 floats, so 4 floats are readable from index 8.
    let c2 = unsafe { vld1q_f32(p2) };
    let p3 = m[12..].as_ptr();
    // SAFETY: `m` has 16 floats, so 4 floats are readable from index 12.
    let c3 = unsafe { vld1q_f32(p3) };

    let raw = objc2::ffi::objc_msgSend as *const ();
    // SAFETY: `objc_msgSend` accepts four `float32x4_t` values in `q0..q3` for `simd_float4x4`.
    let send: unsafe extern "C-unwind" fn(
        *const AnyObject,
        Sel,
        float32x4_t,
        float32x4_t,
        float32x4_t,
        float32x4_t,
    ) = unsafe { core::mem::transmute(raw) };
    // SAFETY: the caller guarantees `obj` is valid and `sel` takes `simd_float4x4`.
    unsafe { send(obj, sel, c0, c1, c2, c3) };
}

/// Sends a 4x4 float matrix setter (`simd_float4x4`) via `objc_msgSend`.
///
/// # Safety
///
/// `obj` must point to a valid Objective-C receiver implementing `sel` taking `simd_float4x4`.
#[cfg(not(target_arch = "aarch64"))]
unsafe fn send_set_float4x4(obj: *const AnyObject, sel: Sel, m: &[f32; 16]) {
    #[repr(C, align(16))]
    struct Mat4([[f32; 4]; 4]);

    type SendFn = unsafe extern "C-unwind" fn(*const AnyObject, Sel, Mat4);

    let mat = Mat4([
        [m[0], m[1], m[2], m[3]],
        [m[4], m[5], m[6], m[7]],
        [m[8], m[9], m[10], m[11]],
        [m[12], m[13], m[14], m[15]],
    ]);
    let raw = objc2::ffi::objc_msgSend as *const ();
    // SAFETY: `objc_msgSend` accepts a 64-byte `Mat4` on the stack on x86_64 SysV ABI.
    let send = unsafe { core::mem::transmute::<*const (), SendFn>(raw) };
    // SAFETY: the caller guarantees `obj` is valid and `sel` takes `simd_float4x4`.
    unsafe { send(obj, sel, mat) };
}

extern_class!(
    /// `MTLFXTemporalScalerDescriptor`, re-declared to send a nil device and the macOS 27 setters.
    #[unsafe(super(NSObject))]
    #[name = "MTLFXTemporalScalerDescriptor"]
    struct TemporalDescriptor;
);

impl TemporalDescriptor {
    extern_methods!(
        #[unsafe(method(init))]
        fn init(this: Allocated<Self>) -> Retained<Self>;
        #[unsafe(method(setColorTextureFormat:))]
        fn set_color_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setOutputTextureFormat:))]
        fn set_output_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setDepthTextureFormat:))]
        fn set_depth_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setMotionTextureFormat:))]
        fn set_motion_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setInputWidth:))]
        fn set_input_width(&self, width: usize);
        #[unsafe(method(setInputHeight:))]
        fn set_input_height(&self, height: usize);
        #[unsafe(method(setOutputWidth:))]
        fn set_output_width(&self, width: usize);
        #[unsafe(method(setOutputHeight:))]
        fn set_output_height(&self, height: usize);
        #[unsafe(method(setInputContentMaxScale:))]
        fn set_input_content_max_scale(&self, scale: f32);
        #[unsafe(method(setInputContentMinScale:))]
        fn set_input_content_min_scale(&self, scale: f32);
        #[unsafe(method(setInputContentPropertiesEnabled:))]
        fn set_input_content_properties_enabled(&self, enabled: bool);
        #[unsafe(method(setRequiresSynchronousInitialization:))]
        fn set_requires_synchronous_initialization(&self, required: bool);
        #[unsafe(method(setAutoExposureEnabled:))]
        fn set_auto_exposure_enabled(&self, enabled: bool);
        #[unsafe(method(setReactiveMaskTextureEnabled:))]
        fn set_reactive_mask_texture_enabled(&self, enabled: bool);
        #[unsafe(method(setReactiveMaskTextureFormat:))]
        fn set_reactive_mask_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setOutputResolutionMotionVectorsEnabled:))]
        fn set_output_resolution_motion_vectors_enabled(&self, enabled: bool);
        #[unsafe(method(setJitteredMotionVectorsEnabled:))]
        fn set_jittered_motion_vectors_enabled(&self, enabled: bool);
        #[unsafe(method(newTemporalScalerWithDevice:))]
        fn new_temporal_scaler(
            &self,
            device: Option<&ProtocolObject<dyn MTLDevice>>,
        ) -> Option<Retained<ProtocolObject<dyn MTLFXTemporalScaler>>>;
        #[unsafe(method(supportsDevice:))]
        fn supports_device(device: Option<&ProtocolObject<dyn MTLDevice>>) -> bool;
        #[unsafe(method(supportedInputContentMinScaleForDevice:))]
        fn supported_input_content_min_scale(device: Option<&ProtocolObject<dyn MTLDevice>>)
        -> f32;
        #[unsafe(method(supportedInputContentMaxScaleForDevice:))]
        fn supported_input_content_max_scale(device: Option<&ProtocolObject<dyn MTLDevice>>)
        -> f32;
    );
}

extern_class!(
    /// `MTLFXSpatialScalerDescriptor`, re-declared to send a nil device.
    #[unsafe(super(NSObject))]
    #[name = "MTLFXSpatialScalerDescriptor"]
    struct SpatialDescriptor;
);

impl SpatialDescriptor {
    extern_methods!(
        #[unsafe(method(init))]
        fn init(this: Allocated<Self>) -> Retained<Self>;
        #[unsafe(method(setColorTextureFormat:))]
        fn set_color_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setOutputTextureFormat:))]
        fn set_output_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setInputWidth:))]
        fn set_input_width(&self, width: usize);
        #[unsafe(method(setInputHeight:))]
        fn set_input_height(&self, height: usize);
        #[unsafe(method(setOutputWidth:))]
        fn set_output_width(&self, width: usize);
        #[unsafe(method(setOutputHeight:))]
        fn set_output_height(&self, height: usize);
        #[unsafe(method(newSpatialScalerWithDevice:))]
        fn new_spatial_scaler(
            &self,
            device: Option<&ProtocolObject<dyn MTLDevice>>,
        ) -> Option<Retained<ProtocolObject<dyn MTLFXSpatialScaler>>>;
        #[unsafe(method(supportsDevice:))]
        fn supports_device(device: Option<&ProtocolObject<dyn MTLDevice>>) -> bool;
    );
}

extern_class!(
    /// `MTLFXFrameInterpolatorDescriptor`, re-declared to send a nil device.
    #[unsafe(super(NSObject))]
    #[name = "MTLFXFrameInterpolatorDescriptor"]
    struct InterpolatorDescriptor;
);

impl InterpolatorDescriptor {
    extern_methods!(
        #[unsafe(method(init))]
        fn init(this: Allocated<Self>) -> Retained<Self>;
        #[unsafe(method(setColorTextureFormat:))]
        fn set_color_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setOutputTextureFormat:))]
        fn set_output_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setDepthTextureFormat:))]
        fn set_depth_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setMotionTextureFormat:))]
        fn set_motion_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setUITextureFormat:))]
        fn set_ui_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setInputWidth:))]
        fn set_input_width(&self, width: usize);
        #[unsafe(method(setInputHeight:))]
        fn set_input_height(&self, height: usize);
        #[unsafe(method(setOutputWidth:))]
        fn set_output_width(&self, width: usize);
        #[unsafe(method(setOutputHeight:))]
        fn set_output_height(&self, height: usize);
        #[unsafe(method(setScaler:))]
        fn set_scaler(&self, scaler: Option<&ProtocolObject<dyn MTLFXFrameInterpolatableScaler>>);
        #[unsafe(method(newFrameInterpolatorWithDevice:))]
        fn new_frame_interpolator(
            &self,
            device: Option<&ProtocolObject<dyn MTLDevice>>,
        ) -> Option<Retained<ProtocolObject<dyn MTLFXFrameInterpolator>>>;
        #[unsafe(method(supportsDevice:))]
        fn supports_device(device: Option<&ProtocolObject<dyn MTLDevice>>) -> bool;
    );
}

/// The sends to a temporal scaler.
trait TemporalScalerSends {
    fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_exposure_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
    fn set_input_content_width(&self, width: usize);
    fn set_input_content_height(&self, height: usize);
    fn set_reset(&self, reset: bool);
    fn set_depth_reversed(&self, reversed: bool);
    fn set_motion_vector_scale_x(&self, scale: f32);
    fn set_motion_vector_scale_y(&self, scale: f32);
    fn set_jitter_offset_x(&self, offset: f32);
    fn set_jitter_offset_y(&self, offset: f32);
    fn set_pre_exposure(&self, exposure: f32);
    fn set_reactive_mask_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_color_content_offset_x(&self, offset: usize);
    fn set_color_content_offset_y(&self, offset: usize);
    fn set_depth_content_offset_x(&self, offset: usize);
    fn set_depth_content_offset_y(&self, offset: usize);
    fn set_motion_content_offset_x(&self, offset: usize);
    fn set_motion_content_offset_y(&self, offset: usize);
    fn set_reactive_mask_content_offset_x(&self, offset: usize);
    fn set_reactive_mask_content_offset_y(&self, offset: usize);
    fn set_output_offset_x(&self, offset: usize);
    fn set_output_offset_y(&self, offset: usize);
    fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
}

impl TemporalScalerSends for ProtocolObject<dyn MTLFXTemporalScaler> {
    extern_methods!(
        #[unsafe(method(setColorTexture:))]
        fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setOutputTexture:))]
        fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setDepthTexture:))]
        fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setMotionTexture:))]
        fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setExposureTexture:))]
        fn set_exposure_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setFence:))]
        fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
        #[unsafe(method(setInputContentWidth:))]
        fn set_input_content_width(&self, width: usize);
        #[unsafe(method(setInputContentHeight:))]
        fn set_input_content_height(&self, height: usize);
        #[unsafe(method(setReset:))]
        fn set_reset(&self, reset: bool);
        #[unsafe(method(setDepthReversed:))]
        fn set_depth_reversed(&self, reversed: bool);
        #[unsafe(method(setMotionVectorScaleX:))]
        fn set_motion_vector_scale_x(&self, scale: f32);
        #[unsafe(method(setMotionVectorScaleY:))]
        fn set_motion_vector_scale_y(&self, scale: f32);
        #[unsafe(method(setJitterOffsetX:))]
        fn set_jitter_offset_x(&self, offset: f32);
        #[unsafe(method(setJitterOffsetY:))]
        fn set_jitter_offset_y(&self, offset: f32);
        #[unsafe(method(setPreExposure:))]
        fn set_pre_exposure(&self, exposure: f32);
        #[unsafe(method(setReactiveMaskTexture:))]
        fn set_reactive_mask_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setColorContentOffsetX:))]
        fn set_color_content_offset_x(&self, offset: usize);
        #[unsafe(method(setColorContentOffsetY:))]
        fn set_color_content_offset_y(&self, offset: usize);
        #[unsafe(method(setDepthContentOffsetX:))]
        fn set_depth_content_offset_x(&self, offset: usize);
        #[unsafe(method(setDepthContentOffsetY:))]
        fn set_depth_content_offset_y(&self, offset: usize);
        #[unsafe(method(setMotionContentOffsetX:))]
        fn set_motion_content_offset_x(&self, offset: usize);
        #[unsafe(method(setMotionContentOffsetY:))]
        fn set_motion_content_offset_y(&self, offset: usize);
        #[unsafe(method(setReactiveMaskContentOffsetX:))]
        fn set_reactive_mask_content_offset_x(&self, offset: usize);
        #[unsafe(method(setReactiveMaskContentOffsetY:))]
        fn set_reactive_mask_content_offset_y(&self, offset: usize);
        #[unsafe(method(setOutputOffsetX:))]
        fn set_output_offset_x(&self, offset: usize);
        #[unsafe(method(setOutputOffsetY:))]
        fn set_output_offset_y(&self, offset: usize);
        #[unsafe(method(encodeToCommandBuffer:))]
        fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
    );
}

/// The sends to a spatial scaler.
trait SpatialScalerSends {
    fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
    fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
}

impl SpatialScalerSends for ProtocolObject<dyn MTLFXSpatialScaler> {
    extern_methods!(
        #[unsafe(method(setColorTexture:))]
        fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setOutputTexture:))]
        fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setFence:))]
        fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
        #[unsafe(method(encodeToCommandBuffer:))]
        fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
    );
}

/// The sends to a frame interpolator.
trait InterpolatorSends {
    fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_prev_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_ui_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
    fn set_motion_vector_scale_x(&self, scale: f32);
    fn set_motion_vector_scale_y(&self, scale: f32);
    fn set_delta_time(&self, time: f32);
    fn set_near_plane(&self, plane: f32);
    fn set_far_plane(&self, plane: f32);
    fn set_field_of_view(&self, field: f32);
    fn set_aspect_ratio(&self, ratio: f32);
    fn set_jitter_offset_x(&self, offset: f32);
    fn set_jitter_offset_y(&self, offset: f32);
    fn set_should_reset_history(&self, reset: bool);
    fn set_depth_reversed(&self, reversed: bool);
    fn set_is_ui_texture_composited(&self, composited: bool);
    fn set_content_width(&self, width: usize);
    fn set_content_height(&self, height: usize);
    fn set_depth_content_offset_x(&self, offset: usize);
    fn set_depth_content_offset_y(&self, offset: usize);
    fn set_motion_content_offset_x(&self, offset: usize);
    fn set_motion_content_offset_y(&self, offset: usize);
    fn set_output_offset_x(&self, offset: usize);
    fn set_output_offset_y(&self, offset: usize);
    fn input_width(&self) -> usize;
    fn input_height(&self) -> usize;
    fn responds_to_selector(&self, sel: Sel) -> bool;
    fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
}

impl InterpolatorSends for ProtocolObject<dyn MTLFXFrameInterpolator> {
    extern_methods!(
        #[unsafe(method(setColorTexture:))]
        fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setPrevColorTexture:))]
        fn set_prev_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setDepthTexture:))]
        fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setMotionTexture:))]
        fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setOutputTexture:))]
        fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setUITexture:))]
        fn set_ui_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setFence:))]
        fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
        #[unsafe(method(setMotionVectorScaleX:))]
        fn set_motion_vector_scale_x(&self, scale: f32);
        #[unsafe(method(setMotionVectorScaleY:))]
        fn set_motion_vector_scale_y(&self, scale: f32);
        #[unsafe(method(setDeltaTime:))]
        fn set_delta_time(&self, time: f32);
        #[unsafe(method(setNearPlane:))]
        fn set_near_plane(&self, plane: f32);
        #[unsafe(method(setFarPlane:))]
        fn set_far_plane(&self, plane: f32);
        #[unsafe(method(setFieldOfView:))]
        fn set_field_of_view(&self, field: f32);
        #[unsafe(method(setAspectRatio:))]
        fn set_aspect_ratio(&self, ratio: f32);
        #[unsafe(method(setJitterOffsetX:))]
        fn set_jitter_offset_x(&self, offset: f32);
        #[unsafe(method(setJitterOffsetY:))]
        fn set_jitter_offset_y(&self, offset: f32);
        #[unsafe(method(setShouldResetHistory:))]
        fn set_should_reset_history(&self, reset: bool);
        #[unsafe(method(setDepthReversed:))]
        fn set_depth_reversed(&self, reversed: bool);
        #[unsafe(method(setIsUITextureComposited:))]
        fn set_is_ui_texture_composited(&self, composited: bool);
        #[unsafe(method(setContentWidth:))]
        fn set_content_width(&self, width: usize);
        #[unsafe(method(setContentHeight:))]
        fn set_content_height(&self, height: usize);
        #[unsafe(method(setDepthContentOffsetX:))]
        fn set_depth_content_offset_x(&self, offset: usize);
        #[unsafe(method(setDepthContentOffsetY:))]
        fn set_depth_content_offset_y(&self, offset: usize);
        #[unsafe(method(setMotionContentOffsetX:))]
        fn set_motion_content_offset_x(&self, offset: usize);
        #[unsafe(method(setMotionContentOffsetY:))]
        fn set_motion_content_offset_y(&self, offset: usize);
        #[unsafe(method(setOutputOffsetX:))]
        fn set_output_offset_x(&self, offset: usize);
        #[unsafe(method(setOutputOffsetY:))]
        fn set_output_offset_y(&self, offset: usize);
        #[unsafe(method(inputWidth))]
        fn input_width(&self) -> usize;
        #[unsafe(method(inputHeight))]
        fn input_height(&self) -> usize;
        #[unsafe(method(respondsToSelector:))]
        fn responds_to_selector(&self, sel: Sel) -> bool;
        #[unsafe(method(encodeToCommandBuffer:))]
        fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
    );
}

extern_class!(
    /// `MTLFXTemporalDenoisedScalerDescriptor`, re-declared for macOS 26 runtime lookup.
    #[unsafe(super(NSObject))]
    #[name = "MTLFXTemporalDenoisedScalerDescriptor"]
    struct TemporalDenoisedDescriptor;
);

impl TemporalDenoisedDescriptor {
    extern_methods!(
        #[unsafe(method(init))]
        fn init(this: Allocated<Self>) -> Retained<Self>;
        #[unsafe(method(setColorTextureFormat:))]
        fn set_color_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setDepthTextureFormat:))]
        fn set_depth_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setMotionTextureFormat:))]
        fn set_motion_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setDiffuseAlbedoTextureFormat:))]
        fn set_diffuse_albedo_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setSpecularAlbedoTextureFormat:))]
        fn set_specular_albedo_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setNormalTextureFormat:))]
        fn set_normal_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setRoughnessTextureFormat:))]
        fn set_roughness_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setSpecularHitDistanceTextureFormat:))]
        fn set_specular_hit_distance_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setDenoiseStrengthMaskTextureFormat:))]
        fn set_denoise_strength_mask_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setTransparencyOverlayTextureFormat:))]
        fn set_transparency_overlay_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setReactiveMaskTextureFormat:))]
        fn set_reactive_mask_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setOutputTextureFormat:))]
        fn set_output_texture_format(&self, format: MTLPixelFormat);
        #[unsafe(method(setInputWidth:))]
        fn set_input_width(&self, width: usize);
        #[unsafe(method(setInputHeight:))]
        fn set_input_height(&self, height: usize);
        #[unsafe(method(setOutputWidth:))]
        fn set_output_width(&self, width: usize);
        #[unsafe(method(setOutputHeight:))]
        fn set_output_height(&self, height: usize);
        #[unsafe(method(setRequiresSynchronousInitialization:))]
        fn set_requires_synchronous_initialization(&self, required: bool);
        #[unsafe(method(setAutoExposureEnabled:))]
        fn set_auto_exposure_enabled(&self, enabled: bool);
        #[unsafe(method(setReactiveMaskTextureEnabled:))]
        fn set_reactive_mask_texture_enabled(&self, enabled: bool);
        #[unsafe(method(setSpecularHitDistanceTextureEnabled:))]
        fn set_specular_hit_distance_texture_enabled(&self, enabled: bool);
        #[unsafe(method(setDenoiseStrengthMaskTextureEnabled:))]
        fn set_denoise_strength_mask_texture_enabled(&self, enabled: bool);
        #[unsafe(method(setTransparencyOverlayTextureEnabled:))]
        fn set_transparency_overlay_texture_enabled(&self, enabled: bool);
        #[unsafe(method(newTemporalDenoisedScalerWithDevice:))]
        fn new_temporal_denoised_scaler(
            &self,
            device: Option<&ProtocolObject<dyn MTLDevice>>,
        ) -> Option<Retained<NSObject>>;
        #[unsafe(method(supportsDevice:))]
        fn supports_device(device: Option<&ProtocolObject<dyn MTLDevice>>) -> bool;
    );
}

/// The sends to a temporal denoised scaler.
trait DenoisedScalerSends {
    fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_diffuse_albedo_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_specular_albedo_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_normal_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_roughness_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_specular_hit_distance_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_denoise_strength_mask_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_transparency_overlay_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_exposure_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_reactive_mask_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
    fn set_pre_exposure(&self, exposure: f32);
    fn set_jitter_offset_x(&self, offset: f32);
    fn set_jitter_offset_y(&self, offset: f32);
    fn set_motion_vector_scale_x(&self, scale: f32);
    fn set_motion_vector_scale_y(&self, scale: f32);
    fn set_should_reset_history(&self, reset: bool);
    fn set_depth_reversed(&self, reversed: bool);
    fn set_input_content_width(&self, width: usize);
    fn set_input_content_height(&self, height: usize);
    fn responds_to_selector(&self, sel: Sel) -> bool;
    fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
}

impl DenoisedScalerSends for NSObject {
    extern_methods!(
        #[unsafe(method(setColorTexture:))]
        fn set_color_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setOutputTexture:))]
        fn set_output_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setDepthTexture:))]
        fn set_depth_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setMotionTexture:))]
        fn set_motion_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setDiffuseAlbedoTexture:))]
        fn set_diffuse_albedo_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setSpecularAlbedoTexture:))]
        fn set_specular_albedo_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setNormalTexture:))]
        fn set_normal_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setRoughnessTexture:))]
        fn set_roughness_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setSpecularHitDistanceTexture:))]
        fn set_specular_hit_distance_texture(
            &self,
            texture: Option<&ProtocolObject<dyn MTLTexture>>,
        );
        #[unsafe(method(setDenoiseStrengthMaskTexture:))]
        fn set_denoise_strength_mask_texture(
            &self,
            texture: Option<&ProtocolObject<dyn MTLTexture>>,
        );
        #[unsafe(method(setTransparencyOverlayTexture:))]
        fn set_transparency_overlay_texture(
            &self,
            texture: Option<&ProtocolObject<dyn MTLTexture>>,
        );
        #[unsafe(method(setExposureTexture:))]
        fn set_exposure_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setReactiveMaskTexture:))]
        fn set_reactive_mask_texture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(setFence:))]
        fn set_fence(&self, fence: Option<&ProtocolObject<dyn MTLFence>>);
        #[unsafe(method(setPreExposure:))]
        fn set_pre_exposure(&self, exposure: f32);
        #[unsafe(method(setJitterOffsetX:))]
        fn set_jitter_offset_x(&self, offset: f32);
        #[unsafe(method(setJitterOffsetY:))]
        fn set_jitter_offset_y(&self, offset: f32);
        #[unsafe(method(setMotionVectorScaleX:))]
        fn set_motion_vector_scale_x(&self, scale: f32);
        #[unsafe(method(setMotionVectorScaleY:))]
        fn set_motion_vector_scale_y(&self, scale: f32);
        #[unsafe(method(setShouldResetHistory:))]
        fn set_should_reset_history(&self, reset: bool);
        #[unsafe(method(setDepthReversed:))]
        fn set_depth_reversed(&self, reversed: bool);
        #[unsafe(method(setInputContentWidth:))]
        fn set_input_content_width(&self, width: usize);
        #[unsafe(method(setInputContentHeight:))]
        fn set_input_content_height(&self, height: usize);
        #[unsafe(method(respondsToSelector:))]
        fn responds_to_selector(&self, sel: Sel) -> bool;
        #[unsafe(method(encodeToCommandBuffer:))]
        fn encode_to_command_buffer(&self, cmdbuf: Option<&ProtocolObject<dyn MTLCommandBuffer>>);
    );
}

/// Slot 56, `MTLDevice_newTemporalScaler`: a temporal scaler for the device, +1.
///
/// The reactive mask's format is set only when the mask is enabled. `ret` is 0 when `MetalFX`
/// makes none.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_newTemporalScaler")]
pub unsafe extern "C-unwind" fn new_temporal_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtldeviceNewfxtemporalscaler>(args) };
    let desc = TemporalDescriptor::init(TemporalDescriptor::alloc());
    // SAFETY: a null info stops here, where the C faulted; a non-null one is the caller's, live.
    let Some(info) = (unsafe { ffi::mem_ref::<WMTFXTemporalScalerInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    desc.set_color_texture_format(pixel_format(info.color_format));
    desc.set_output_texture_format(pixel_format(info.output_format));
    desc.set_depth_texture_format(pixel_format(info.depth_format));
    desc.set_motion_texture_format(pixel_format(info.motion_format));
    desc.set_input_width(info.input_width as usize);
    desc.set_input_height(info.input_height as usize);
    desc.set_output_width(info.output_width as usize);
    desc.set_output_height(info.output_height as usize);
    desc.set_input_content_max_scale(info.input_content_max_scale);
    desc.set_input_content_min_scale(info.input_content_min_scale);
    desc.set_input_content_properties_enabled(info.input_content_properties_enabled != 0);
    desc.set_requires_synchronous_initialization(info.requires_synchronous_initialization != 0);
    desc.set_auto_exposure_enabled(info.auto_exposure != 0);
    desc.set_reactive_mask_texture_enabled(info.reactive_mask_enabled != 0);
    if info.reactive_mask_enabled != 0 {
        desc.set_reactive_mask_texture_format(pixel_format(info.reactive_mask_format));
    }
    desc.set_output_resolution_motion_vectors_enabled(info.output_resolution_motion_vectors != 0);
    desc.set_jittered_motion_vectors_enabled(info.jittered_motion_vectors != 0);
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    p.ret = handle::ret_owned(desc.new_temporal_scaler(device));
    STATUS_SUCCESS
}

/// Slot 57, `MTLDevice_newSpatialScaler`: a spatial scaler for the device, +1.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_newSpatialScaler")]
pub unsafe extern "C-unwind" fn new_spatial_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtldeviceNewfxspatialscaler>(args) };
    let desc = SpatialDescriptor::init(SpatialDescriptor::alloc());
    // SAFETY: a null info stops here, where the C faulted; a non-null one is the caller's, live.
    let Some(info) = (unsafe { ffi::mem_ref::<WMTFXSpatialScalerInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    desc.set_color_texture_format(pixel_format(info.color_format));
    desc.set_output_texture_format(pixel_format(info.output_format));
    desc.set_input_width(info.input_width as usize);
    desc.set_input_height(info.input_height as usize);
    desc.set_output_width(info.output_width as usize);
    desc.set_output_height(info.output_height as usize);
    // SAFETY: the caller's handle: null or a live object; `MetalFX` raises on a nil device.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    p.ret = handle::ret_owned(desc.new_spatial_scaler(device));
    STATUS_SUCCESS
}

/// Slot 58, `MTLCommandBuffer_encodeTemporalScale`: sets the scaler's 26 properties and encodes.
///
/// A null `props` stops after the six texture and fence setters, where the C faulted.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCommandBuffer_encodeTemporalScale")]
pub unsafe extern "C-unwind" fn encode_temporal_scale(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtlcommandbufferTemporalScale>(args) };
    // SAFETY: the handles are the caller's: null or live objects; Metal raises when the first
    // selector reaches one of the wrong class, as it did for the C.
    let scaler = unsafe { handle::obj::<ProtocolObject<dyn MTLFXTemporalScaler>>(p.scaler) };
    let Some(scaler) = scaler else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let color = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.color) };
    // SAFETY: as above.
    let output = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.output) };
    // SAFETY: as above.
    let depth = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.depth) };
    // SAFETY: as above.
    let motion = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.motion) };
    // SAFETY: as above.
    let exposure = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.exposure) };
    // SAFETY: as above.
    let fence = unsafe { handle::obj::<ProtocolObject<dyn MTLFence>>(p.fence) };
    scaler.set_color_texture(color);
    scaler.set_output_texture(output);
    scaler.set_depth_texture(depth);
    scaler.set_motion_texture(motion);
    scaler.set_exposure_texture(exposure);
    scaler.set_fence(fence);
    // SAFETY: a null props stops here, where the C faulted; a non-null one is the caller's, live.
    let Some(props) = (unsafe { ffi::mem_ref::<WMTFXTemporalScalerProps>(p.props) }) else {
        return STATUS_SUCCESS;
    };
    scaler.set_input_content_width(props.input_content_width as usize);
    scaler.set_input_content_height(props.input_content_height as usize);
    scaler.set_reset(props.reset != 0);
    scaler.set_depth_reversed(props.depth_reversed != 0);
    scaler.set_motion_vector_scale_x(props.motion_vector_scale_x);
    scaler.set_motion_vector_scale_y(props.motion_vector_scale_y);
    scaler.set_jitter_offset_x(props.jitter_offset_x);
    scaler.set_jitter_offset_y(props.jitter_offset_y);
    scaler.set_pre_exposure(props.pre_exposure);
    // SAFETY: as above.
    let reactive_mask = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.reactive_mask) };
    scaler.set_reactive_mask_texture(reactive_mask);
    scaler.set_color_content_offset_x(props.color_content_offset_x as usize);
    scaler.set_color_content_offset_y(props.color_content_offset_y as usize);
    scaler.set_depth_content_offset_x(props.depth_content_offset_x as usize);
    scaler.set_depth_content_offset_y(props.depth_content_offset_y as usize);
    scaler.set_motion_content_offset_x(props.motion_content_offset_x as usize);
    scaler.set_motion_content_offset_y(props.motion_content_offset_y as usize);
    scaler.set_reactive_mask_content_offset_x(props.reactive_mask_content_offset_x as usize);
    scaler.set_reactive_mask_content_offset_y(props.reactive_mask_content_offset_y as usize);
    scaler.set_output_offset_x(props.output_offset_x as usize);
    scaler.set_output_offset_y(props.output_offset_y as usize);
    // SAFETY: as above.
    let cmdbuf = unsafe { handle::obj::<ProtocolObject<dyn MTLCommandBuffer>>(p.cmdbuf) };
    scaler.encode_to_command_buffer(cmdbuf);
    STATUS_SUCCESS
}

/// Slot 59, `MTLCommandBuffer_encodeSpatialScale`: sets the scaler's three properties and encodes.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCommandBuffer_encodeSpatialScale")]
pub unsafe extern "C-unwind" fn encode_spatial_scale(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtlcommandbufferSpatialScale>(args) };
    // SAFETY: the handles are the caller's: null or live objects; Metal raises when the first
    // selector reaches one of the wrong class, as it did for the C.
    let scaler = unsafe { handle::obj::<ProtocolObject<dyn MTLFXSpatialScaler>>(p.scaler) };
    let Some(scaler) = scaler else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let color = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.color) };
    // SAFETY: as above.
    let output = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.output) };
    // SAFETY: as above.
    let fence = unsafe { handle::obj::<ProtocolObject<dyn MTLFence>>(p.fence) };
    // SAFETY: as above.
    let cmdbuf = unsafe { handle::obj::<ProtocolObject<dyn MTLCommandBuffer>>(p.cmdbuf) };
    scaler.set_color_texture(color);
    scaler.set_output_texture(output);
    scaler.set_fence(fence);
    scaler.encode_to_command_buffer(cmdbuf);
    STATUS_SUCCESS
}

/// Slot 68, `MTLDevice_supportsFXSpatialScaler`: whether `MetalFX` scales spatially there.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsFXSpatialScaler")]
pub unsafe extern "C-unwind" fn supports_fx_spatial_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = u64::from(SpatialDescriptor::supports_device(device));
    STATUS_SUCCESS
}

/// Slot 69, `MTLDevice_supportsFXTemporalScaler`: whether `MetalFX` scales temporally there.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsFXTemporalScaler")]
pub unsafe extern "C-unwind" fn supports_fx_temporal_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = u64::from(TemporalDescriptor::supports_device(device));
    STATUS_SUCCESS
}

/// Slot 150, `MTLDevice_temporalScalerScaleRange`: the input content scales the device supports.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_temporalScalerScaleRange")]
pub unsafe extern "C-unwind" fn temporal_scaler_scale_range(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtldeviceFxtemporalscalerScale>(args) };
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    p.min_scale = TemporalDescriptor::supported_input_content_min_scale(device);
    p.max_scale = TemporalDescriptor::supported_input_content_max_scale(device);
    STATUS_SUCCESS
}

/// Slot 151, `MTLDevice_newFrameInterpolator`: a frame interpolator for the device, +1.
///
/// `ret` is 0 when `MetalFX` does not support the device, or makes none. The UI texture format is
/// set from `info.ui_format`; the scaler is set when the handle is not null.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_newFrameInterpolator")]
pub unsafe extern "C-unwind" fn new_frame_interpolator(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtldeviceNewfxframeinterpolator>(args) };
    p.ret = 0;
    // SAFETY: a null info stops here, where the C faulted; a non-null one is the caller's, live.
    let Some(info) = (unsafe { ffi::mem_ref::<WMTFXFrameInterpolatorInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let desc = InterpolatorDescriptor::init(InterpolatorDescriptor::alloc());
    desc.set_color_texture_format(pixel_format(info.color_format));
    desc.set_output_texture_format(pixel_format(info.output_format));
    desc.set_depth_texture_format(pixel_format(info.depth_format));
    desc.set_motion_texture_format(pixel_format(info.motion_format));
    desc.set_ui_texture_format(pixel_format(info.ui_format));
    desc.set_input_width(info.input_width as usize);
    desc.set_input_height(info.input_height as usize);
    desc.set_output_width(info.output_width as usize);
    desc.set_output_height(info.output_height as usize);
    // SAFETY: the caller's handle: null or a live object; `MetalFX` raises on a wrong class.
    let scaler =
        unsafe { handle::obj::<ProtocolObject<dyn MTLFXFrameInterpolatableScaler>>(info.scaler) };
    if scaler.is_some() {
        desc.set_scaler(scaler);
    }
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    if InterpolatorDescriptor::supports_device(device) {
        p.ret = handle::ret_owned(desc.new_frame_interpolator(device));
    }
    STATUS_SUCCESS
}

fn apply_interpolator_macos27(
    interpolator: &ProtocolObject<dyn MTLFXFrameInterpolator>,
    props: &WMTFXFrameInterpolatorProps,
) {
    if interpolator.responds_to_selector(Sel::register(c"setContentWidth:")) {
        let width = if props.content_width != 0 {
            props.content_width as usize
        } else if interpolator.responds_to_selector(Sel::register(c"inputWidth")) {
            interpolator.input_width()
        } else {
            0
        };
        interpolator.set_content_width(width);
    }
    if interpolator.responds_to_selector(Sel::register(c"setContentHeight:")) {
        let height = if props.content_height != 0 {
            props.content_height as usize
        } else if interpolator.responds_to_selector(Sel::register(c"inputHeight")) {
            interpolator.input_height()
        } else {
            0
        };
        interpolator.set_content_height(height);
    }
    if interpolator.responds_to_selector(Sel::register(c"setDepthContentOffsetX:")) {
        interpolator.set_depth_content_offset_x(props.depth_content_offset_x as usize);
    }
    if interpolator.responds_to_selector(Sel::register(c"setDepthContentOffsetY:")) {
        interpolator.set_depth_content_offset_y(props.depth_content_offset_y as usize);
    }
    if interpolator.responds_to_selector(Sel::register(c"setMotionContentOffsetX:")) {
        interpolator.set_motion_content_offset_x(props.motion_content_offset_x as usize);
    }
    if interpolator.responds_to_selector(Sel::register(c"setMotionContentOffsetY:")) {
        interpolator.set_motion_content_offset_y(props.motion_content_offset_y as usize);
    }
    if interpolator.responds_to_selector(Sel::register(c"setOutputOffsetX:")) {
        interpolator.set_output_offset_x(props.output_offset_x as usize);
    }
    if interpolator.responds_to_selector(Sel::register(c"setOutputOffsetY:")) {
        interpolator.set_output_offset_y(props.output_offset_y as usize);
    }
    if props.has_matrices != 0 {
        let sel_w2v = Sel::register(c"setWorldToViewMatrix:");
        if interpolator.responds_to_selector(sel_w2v) {
            // SAFETY: `interpolator` is a live `MTLFXFrameInterpolator` and responds to `sel_w2v`.
            unsafe {
                send_set_float4x4(
                    core::ptr::from_ref(interpolator).cast(),
                    sel_w2v,
                    &props.world_to_view_matrix,
                );
            }
        }
        let sel_v2c = Sel::register(c"setViewToClipMatrix:");
        if interpolator.responds_to_selector(sel_v2c) {
            // SAFETY: `interpolator` is a live `MTLFXFrameInterpolator` and responds to `sel_v2c`.
            unsafe {
                send_set_float4x4(
                    core::ptr::from_ref(interpolator).cast(),
                    sel_v2c,
                    &props.view_to_clip_matrix,
                );
            }
        }
    }
}

/// Slot 152, `MTLCommandBuffer_encodeFrameInterpolate`: sets properties and encodes.
///
/// A null `props` stops after the seven texture and fence setters, where the C faulted. Optional
/// macOS 27 selectors are gated on `respondsToSelector:` so macOS 26 hosts never crash.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCommandBuffer_encodeFrameInterpolate")]
pub unsafe extern "C-unwind" fn encode_frame_interpolate(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtlcommandbufferFrameInterpolate>(args) };
    // SAFETY: the handles are the caller's: null or live objects; Metal raises when the first
    // selector reaches one of the wrong class, as it did for the C.
    let interpolator =
        unsafe { handle::obj::<ProtocolObject<dyn MTLFXFrameInterpolator>>(p.interpolator) };
    let Some(interpolator) = interpolator else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let color = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.color) };
    // SAFETY: as above.
    let prev_color = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.prev_color) };
    // SAFETY: as above.
    let depth = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.depth) };
    // SAFETY: as above.
    let motion = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.motion) };
    // SAFETY: as above.
    let output = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.output) };
    // SAFETY: as above.
    let ui_texture = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.ui_texture) };
    // SAFETY: as above.
    let fence = unsafe { handle::obj::<ProtocolObject<dyn MTLFence>>(p.fence) };
    interpolator.set_color_texture(color);
    interpolator.set_prev_color_texture(prev_color);
    interpolator.set_depth_texture(depth);
    interpolator.set_motion_texture(motion);
    interpolator.set_output_texture(output);
    interpolator.set_ui_texture(ui_texture);
    interpolator.set_fence(fence);
    // SAFETY: a null props stops here, where the C faulted; a non-null one is the caller's, live.
    let Some(props) = (unsafe { ffi::mem_ref::<WMTFXFrameInterpolatorProps>(p.props) }) else {
        return STATUS_SUCCESS;
    };
    interpolator.set_motion_vector_scale_x(props.motion_vector_scale_x);
    interpolator.set_motion_vector_scale_y(props.motion_vector_scale_y);
    interpolator.set_delta_time(props.delta_time);
    interpolator.set_near_plane(props.near_plane);
    interpolator.set_far_plane(props.far_plane);
    interpolator.set_field_of_view(props.field_of_view);
    interpolator.set_aspect_ratio(props.aspect_ratio);
    interpolator.set_jitter_offset_x(props.jitter_offset_x);
    interpolator.set_jitter_offset_y(props.jitter_offset_y);
    interpolator.set_should_reset_history(props.reset != 0);
    interpolator.set_depth_reversed(props.depth_reversed != 0);
    interpolator.set_is_ui_texture_composited(props.ui_composited != 0);
    apply_interpolator_macos27(interpolator, props);
    // SAFETY: as above.
    let cmdbuf = unsafe { handle::obj::<ProtocolObject<dyn MTLCommandBuffer>>(p.cmdbuf) };
    interpolator.encode_to_command_buffer(cmdbuf);
    STATUS_SUCCESS
}

/// Slot 165, `MTLDevice_supportsFXTemporalDenoisedScaler`: whether `MetalFX` denoises and scales.
///
/// Returns `0` on macOS releases lacking `MTLFXTemporalDenoisedScalerDescriptor`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsFXTemporalDenoisedScaler")]
pub unsafe extern "C-unwind" fn supports_fx_temporal_denoised_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallGenericObjUint64Ret>(args) };
    if handle::lookup_class(c"MTLFXTemporalDenoisedScalerDescriptor").is_none() {
        p.ret = 0;
        return STATUS_SUCCESS;
    }
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = u64::from(TemporalDenoisedDescriptor::supports_device(device));
    STATUS_SUCCESS
}

/// Slot 166, `MTLDevice_newTemporalDenoisedScaler`: a denoised scaler for the device, +1.
///
/// `ret` is 0 when `MetalFX` lacks the class, does not support the device, or makes none.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_newTemporalDenoisedScaler")]
pub unsafe extern "C-unwind" fn new_temporal_denoised_scaler(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtldeviceNewfxtemporaldenoisedscaler>(args) };
    p.ret = 0;
    if handle::lookup_class(c"MTLFXTemporalDenoisedScalerDescriptor").is_none() {
        return STATUS_SUCCESS;
    }
    // SAFETY: a null info stops here; a non-null one is the caller's, live.
    let Some(info) = (unsafe { ffi::mem_ref::<WMTFXTemporalDenoisedScalerInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let desc = TemporalDenoisedDescriptor::init(TemporalDenoisedDescriptor::alloc());
    desc.set_color_texture_format(pixel_format(info.color_format));
    desc.set_depth_texture_format(pixel_format(info.depth_format));
    desc.set_motion_texture_format(pixel_format(info.motion_format));
    desc.set_diffuse_albedo_texture_format(pixel_format(info.diffuse_albedo_format));
    desc.set_specular_albedo_texture_format(pixel_format(info.specular_albedo_format));
    desc.set_normal_texture_format(pixel_format(info.normal_format));
    desc.set_roughness_texture_format(pixel_format(info.roughness_format));
    desc.set_output_texture_format(pixel_format(info.output_format));
    desc.set_input_width(info.input_width as usize);
    desc.set_input_height(info.input_height as usize);
    desc.set_output_width(info.output_width as usize);
    desc.set_output_height(info.output_height as usize);
    desc.set_requires_synchronous_initialization(info.synchronous_init != 0);
    desc.set_auto_exposure_enabled(info.auto_exposure != 0);
    desc.set_reactive_mask_texture_enabled(info.reactive_mask_enabled != 0);
    if info.reactive_mask_enabled != 0 {
        desc.set_reactive_mask_texture_format(pixel_format(info.reactive_mask_format));
    }
    desc.set_specular_hit_distance_texture_enabled(info.specular_hit_distance_enabled != 0);
    if info.specular_hit_distance_enabled != 0 {
        desc.set_specular_hit_distance_texture_format(pixel_format(
            info.specular_hit_distance_format,
        ));
    }
    desc.set_denoise_strength_mask_texture_enabled(info.denoise_strength_mask_enabled != 0);
    if info.denoise_strength_mask_enabled != 0 {
        desc.set_denoise_strength_mask_texture_format(pixel_format(
            info.denoise_strength_mask_format,
        ));
    }
    desc.set_transparency_overlay_texture_enabled(info.transparency_overlay_enabled != 0);
    if info.transparency_overlay_enabled != 0 {
        desc.set_transparency_overlay_texture_format(pixel_format(
            info.transparency_overlay_format,
        ));
    }
    // SAFETY: the caller's handle: null or a live object; a nil device is sent on as nil.
    let device = unsafe { handle::obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    if TemporalDenoisedDescriptor::supports_device(device) {
        p.ret = handle::ret_owned(desc.new_temporal_denoised_scaler(device));
    }
    STATUS_SUCCESS
}

/// Slot 167, `MTLCommandBuffer_encodeTemporalDenoisedScale`: sets properties and encodes.
///
/// A null `props` stops after setting all texture and fence properties.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCommandBuffer_encodeTemporalDenoisedScale")]
pub unsafe extern "C-unwind" fn encode_temporal_denoised_scale(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { ffi::params::<UnixcallMtlcommandbufferTemporalDenoisedScale>(args) };
    // SAFETY: the handles are the caller's: null or live objects; Metal raises when the first
    // selector reaches one of the wrong class.
    let scaler = unsafe { handle::obj::<NSObject>(p.scaler) };
    let Some(scaler) = scaler else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let color = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.color) };
    // SAFETY: as above.
    let output = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.output) };
    // SAFETY: as above.
    let depth = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.depth) };
    // SAFETY: as above.
    let motion = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.motion) };
    // SAFETY: as above.
    let diffuse_albedo = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.diffuse_albedo) };
    // SAFETY: as above.
    let specular_albedo =
        unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.specular_albedo) };
    // SAFETY: as above.
    let normal = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.normal) };
    // SAFETY: as above.
    let roughness = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.roughness) };
    // SAFETY: as above.
    let specular_hit_distance =
        unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.specular_hit_distance) };
    // SAFETY: as above.
    let denoise_strength_mask =
        unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.denoise_strength_mask) };
    // SAFETY: as above.
    let transparency_overlay =
        unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.transparency_overlay) };
    // SAFETY: as above.
    let exposure = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.exposure) };
    // SAFETY: as above.
    let reactive_mask = unsafe { handle::obj::<ProtocolObject<dyn MTLTexture>>(p.reactive_mask) };
    // SAFETY: as above.
    let fence = unsafe { handle::obj::<ProtocolObject<dyn MTLFence>>(p.fence) };
    scaler.set_color_texture(color);
    scaler.set_output_texture(output);
    scaler.set_depth_texture(depth);
    scaler.set_motion_texture(motion);
    scaler.set_diffuse_albedo_texture(diffuse_albedo);
    scaler.set_specular_albedo_texture(specular_albedo);
    scaler.set_normal_texture(normal);
    scaler.set_roughness_texture(roughness);
    scaler.set_specular_hit_distance_texture(specular_hit_distance);
    scaler.set_denoise_strength_mask_texture(denoise_strength_mask);
    scaler.set_transparency_overlay_texture(transparency_overlay);
    scaler.set_exposure_texture(exposure);
    scaler.set_reactive_mask_texture(reactive_mask);
    scaler.set_fence(fence);
    // SAFETY: a null props stops here; a non-null one is the caller's, live.
    let Some(props) = (unsafe { ffi::mem_ref::<WMTFXTemporalDenoisedScalerProps>(p.props) }) else {
        return STATUS_SUCCESS;
    };
    scaler.set_pre_exposure(props.pre_exposure);
    scaler.set_jitter_offset_x(props.jitter_offset_x);
    scaler.set_jitter_offset_y(props.jitter_offset_y);
    scaler.set_motion_vector_scale_x(props.motion_vector_scale_x);
    scaler.set_motion_vector_scale_y(props.motion_vector_scale_y);
    scaler.set_should_reset_history(props.reset != 0);
    scaler.set_depth_reversed(props.depth_reversed != 0);
    if props.input_content_width != 0
        && scaler.responds_to_selector(Sel::register(c"setInputContentWidth:"))
    {
        scaler.set_input_content_width(props.input_content_width as usize);
    }
    if props.input_content_height != 0
        && scaler.responds_to_selector(Sel::register(c"setInputContentHeight:"))
    {
        scaler.set_input_content_height(props.input_content_height as usize);
    }
    if props.has_matrices != 0 {
        // SAFETY: `scaler` is a live `MTLFXTemporalDenoisedScaler` supporting matrix setters.
        unsafe {
            send_set_float4x4(
                core::ptr::from_ref(scaler).cast(),
                Sel::register(c"setWorldToViewMatrix:"),
                &props.world_to_view_matrix,
            );
        }
        // SAFETY: `scaler` is a live `MTLFXTemporalDenoisedScaler` supporting matrix setters.
        unsafe {
            send_set_float4x4(
                core::ptr::from_ref(scaler).cast(),
                Sel::register(c"setViewToClipMatrix:"),
                &props.view_to_clip_matrix,
            );
        }
    }
    // SAFETY: as above.
    let cmdbuf = unsafe { handle::obj::<ProtocolObject<dyn MTLCommandBuffer>>(p.cmdbuf) };
    scaler.encode_to_command_buffer(cmdbuf);
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests;
