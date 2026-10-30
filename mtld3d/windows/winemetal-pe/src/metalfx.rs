//! `MetalFX`: scalers and the frame interpolator.

use winemetal_wire::{
    UnixcallGenericObjUint64Ret, UnixcallMtlcommandbufferFrameInterpolate, UnixcallMtlcommandbufferSpatialScale,
    UnixcallMtlcommandbufferTemporalScale, UnixcallMtldeviceFxtemporalscalerScale,
    UnixcallMtldeviceNewfxframeinterpolator, UnixcallMtldeviceNewfxspatialscaler, UnixcallMtldeviceNewfxtemporalscaler,
    WMTFXFrameInterpolatorInfo, WMTFXFrameInterpolatorProps, WMTFXSpatialScalerInfo, WMTFXTemporalScalerInfo,
    WMTFXTemporalScalerProps, slot,
};

use crate::transport::{call, mem_ptr};

exports! {
    fn MTLDevice_newTemporalScaler(device: u64, info: *const WMTFXTemporalScalerInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_TEMPORAL_SCALER, |p: &mut UnixcallMtldeviceNewfxtemporalscaler| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLDevice_newSpatialScaler(device: u64, info: *const WMTFXSpatialScalerInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_SPATIAL_SCALER, |p: &mut UnixcallMtldeviceNewfxspatialscaler| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLCommandBuffer_encodeTemporalScale(
        cmdbuf: u64,
        scaler: u64,
        color: u64,
        output: u64,
        depth: u64,
        motion: u64,
        exposure: u64,
        fence: u64,
        props: *const WMTFXTemporalScalerProps,
        reactive_mask: u64,
    ) {
        call(slot::MTL_COMMAND_BUFFER_ENCODE_TEMPORAL_SCALE, |p: &mut UnixcallMtlcommandbufferTemporalScale| {
            p.cmdbuf = cmdbuf;
            p.scaler = scaler;
            p.color = color;
            p.output = output;
            p.depth = depth;
            p.motion = motion;
            p.exposure = exposure;
            p.fence = fence;
            p.props = mem_ptr(props);
            p.reactive_mask = reactive_mask;
        });
    }

    fn MTLCommandBuffer_encodeSpatialScale(cmdbuf: u64, scaler: u64, color: u64, output: u64, fence: u64) {
        call(slot::MTL_COMMAND_BUFFER_ENCODE_SPATIAL_SCALE, |p: &mut UnixcallMtlcommandbufferSpatialScale| {
            p.cmdbuf = cmdbuf;
            p.scaler = scaler;
            p.color = color;
            p.output = output;
            p.fence = fence;
        });
    }

    fn MTLDevice_supportsFXSpatialScaler(device: u64) -> bool {
        call(slot::MTL_DEVICE_SUPPORTS_FX_SPATIAL_SCALER, |p: &mut UnixcallGenericObjUint64Ret| p.handle = device).ret
            != 0
    }

    fn MTLDevice_supportsFXTemporalScaler(device: u64) -> bool {
        call(slot::MTL_DEVICE_SUPPORTS_FX_TEMPORAL_SCALER, |p: &mut UnixcallGenericObjUint64Ret| p.handle = device).ret
            != 0
    }

    fn MTLDevice_temporalScalerScaleRange(device: u64, min: *mut f32, max: *mut f32) {
        let p = call(slot::MTL_DEVICE_TEMPORAL_SCALER_SCALE_RANGE, |p: &mut UnixcallMtldeviceFxtemporalscalerScale| {
            p.device = device;
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { min.write(p.min_scale) };
        // SAFETY: as above.
        unsafe { max.write(p.max_scale) };
    }

    fn MTLDevice_newFrameInterpolator(device: u64, info: *const WMTFXFrameInterpolatorInfo) -> u64 {
        call(slot::MTL_DEVICE_NEW_FRAME_INTERPOLATOR, |p: &mut UnixcallMtldeviceNewfxframeinterpolator| {
            p.device = device;
            p.info = mem_ptr(info);
        })
        .ret
    }

    fn MTLCommandBuffer_encodeFrameInterpolate(
        cmdbuf: u64,
        interpolator: u64,
        color: u64,
        prev_color: u64,
        depth: u64,
        motion: u64,
        output: u64,
        fence: u64,
        props: *const WMTFXFrameInterpolatorProps,
    ) {
        call(slot::MTL_COMMAND_BUFFER_ENCODE_FRAME_INTERPOLATE, |p: &mut UnixcallMtlcommandbufferFrameInterpolate| {
            p.cmdbuf = cmdbuf;
            p.interpolator = interpolator;
            p.color = color;
            p.prev_color = prev_color;
            p.depth = depth;
            p.motion = motion;
            p.output = output;
            p.fence = fence;
            p.props = mem_ptr(props);
        });
    }
}
