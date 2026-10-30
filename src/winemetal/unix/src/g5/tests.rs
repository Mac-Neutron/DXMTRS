use objc2_metal::{
    MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandQueue, MTLCreateSystemDefaultDevice,
    MTLStorageMode, MTLTextureDescriptor, MTLTextureUsage,
};
use winemetal_wire::{
    MemPtr, UnixcallMtlcommandbufferFrameInterpolate, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_R8_UNORM, WMT_PIXEL_FORMAT_RG16_FLOAT, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

use super::*;
use crate::float_cast::u64_to_usize;

/// Declares a trait of safe getters on a protocol object, to read back what a handler set.
macro_rules! getters {
    ($trait:ident for $proto:ty { $($sel:ident $name:ident -> $ty:ty;)* }) => {
        trait $trait {
            $(fn $name(&self) -> $ty;)*
        }

        impl $trait for ProtocolObject<$proto> {
            extern_methods!($(#[unsafe(method($sel))] fn $name(&self) -> $ty;)*);
        }
    };
}

getters!(TemporalGetters for dyn MTLFXTemporalScaler {
    inputWidth input_width -> usize;
    inputHeight input_height -> usize;
    outputWidth output_width -> usize;
    outputHeight output_height -> usize;
    colorTextureFormat color_texture_format -> MTLPixelFormat;
    reactiveMaskTextureFormat reactive_mask_texture_format -> MTLPixelFormat;
    colorTextureUsage color_texture_usage -> MTLTextureUsage;
    depthTextureUsage depth_texture_usage -> MTLTextureUsage;
    motionTextureUsage motion_texture_usage -> MTLTextureUsage;
    reactiveMaskTextureUsage reactive_mask_texture_usage -> MTLTextureUsage;
    outputTextureUsage output_texture_usage -> MTLTextureUsage;
    inputContentWidth input_content_width -> usize;
    inputContentHeight input_content_height -> usize;
    depthReversed depth_reversed -> bool;
    motionVectorScaleX motion_vector_scale_x -> f32;
    motionVectorScaleY motion_vector_scale_y -> f32;
    jitterOffsetX jitter_offset_x -> f32;
    jitterOffsetY jitter_offset_y -> f32;
    preExposure pre_exposure -> f32;
    colorContentOffsetX color_content_offset_x -> usize;
    colorContentOffsetY color_content_offset_y -> usize;
    depthContentOffsetX depth_content_offset_x -> usize;
    depthContentOffsetY depth_content_offset_y -> usize;
    motionContentOffsetX motion_content_offset_x -> usize;
    motionContentOffsetY motion_content_offset_y -> usize;
    reactiveMaskContentOffsetX reactive_mask_content_offset_x -> usize;
    reactiveMaskContentOffsetY reactive_mask_content_offset_y -> usize;
    outputOffsetX output_offset_x -> usize;
    outputOffsetY output_offset_y -> usize;
    colorTexture color_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    outputTexture output_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    depthTexture depth_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    motionTexture motion_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    exposureTexture exposure_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    reactiveMaskTexture reactive_mask_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    fence fence -> Option<Retained<ProtocolObject<dyn MTLFence>>>;
});

getters!(SpatialGetters for dyn MTLFXSpatialScaler {
    inputWidth input_width -> usize;
    inputHeight input_height -> usize;
    outputWidth output_width -> usize;
    outputHeight output_height -> usize;
    colorTextureUsage color_texture_usage -> MTLTextureUsage;
    outputTextureUsage output_texture_usage -> MTLTextureUsage;
    colorTexture color_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    outputTexture output_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    fence fence -> Option<Retained<ProtocolObject<dyn MTLFence>>>;
});

getters!(InterpolatorGetters for dyn MTLFXFrameInterpolator {
    colorTextureUsage color_texture_usage -> MTLTextureUsage;
    depthTextureUsage depth_texture_usage -> MTLTextureUsage;
    motionTextureUsage motion_texture_usage -> MTLTextureUsage;
    outputTextureUsage output_texture_usage -> MTLTextureUsage;
    motionVectorScaleX motion_vector_scale_x -> f32;
    motionVectorScaleY motion_vector_scale_y -> f32;
    deltaTime delta_time -> f32;
    nearPlane near_plane -> f32;
    farPlane far_plane -> f32;
    fieldOfView field_of_view -> f32;
    aspectRatio aspect_ratio -> f32;
    jitterOffsetX jitter_offset_x -> f32;
    jitterOffsetY jitter_offset_y -> f32;
    isDepthReversed depth_reversed -> bool;
    isUITextureComposited ui_texture_composited -> bool;
    colorTexture color_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    prevColorTexture prev_color_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    depthTexture depth_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    motionTexture motion_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    outputTexture output_texture -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    fence fence -> Option<Retained<ProtocolObject<dyn MTLFence>>>;
});

fn device() -> Retained<ProtocolObject<dyn MTLDevice>> {
    MTLCreateSystemDefaultDevice().expect("a Metal device")
}

/// An object's address as a handle.
fn handle_of<T>(o: &T) -> u64 {
    core::ptr::from_ref(o).expose_provenance() as u64
}

fn mem<T>(r: &T) -> MemPtr {
    MemPtr { ptr: handle_of(r) }
}

/// Runs a handler on a params block.
fn call<T>(handler: unsafe extern "C-unwind" fn(*mut c_void) -> i32, params: &mut T) -> i32 {
    // SAFETY: `params` is the type the handler's slot takes, live and unshared.
    unsafe { handler(core::ptr::from_mut(params).cast()) }
}

/// The +1 object a handler answered.
fn owned<T: objc2::Message>(h: u64) -> Retained<T> {
    let raw = core::ptr::with_exposed_provenance_mut(u64_to_usize(h));
    // SAFETY: a handler's `ret` is a +1 object of the class its slot makes.
    unsafe { Retained::from_raw(raw) }.expect("a +1 handle")
}

fn texture(
    device: &ProtocolObject<dyn MTLDevice>,
    format: MTLPixelFormat,
    side: usize,
    usage: MTLTextureUsage,
) -> Retained<ProtocolObject<dyn MTLTexture>> {
    // SAFETY: a descriptor factory taking plain values.
    let desc = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            format, side, side, false,
        )
    };
    desc.setUsage(usage);
    desc.setStorageMode(MTLStorageMode::Private);
    device.newTextureWithDescriptor(&desc).expect("a texture")
}

fn same_object<T>(got: Option<&T>, want: &T) -> bool {
    got.is_some_and(|got| core::ptr::eq(got, want))
}

fn same(got: f32, want: f32) {
    assert_eq!(got.to_bits(), want.to_bits(), "{got} against {want}");
}

const RGBA16F: u32 = WMT_PIXEL_FORMAT_RGBA16_FLOAT;

fn temporal_info(reactive_mask: bool) -> WMTFXTemporalScalerInfo {
    WMTFXTemporalScalerInfo {
        color_format: RGBA16F,
        output_format: RGBA16F,
        depth_format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        motion_format: WMT_PIXEL_FORMAT_RG16_FLOAT,
        input_width: 64,
        input_height: 48,
        output_width: 128,
        output_height: 96,
        input_content_min_scale: 1.0,
        input_content_max_scale: 2.0,
        auto_exposure: 0,
        input_content_properties_enabled: 0,
        requires_synchronous_initialization: 0,
        reactive_mask_enabled: u8::from(reactive_mask),
        reactive_mask_format: WMT_PIXEL_FORMAT_R8_UNORM,
        output_resolution_motion_vectors: 0,
        jittered_motion_vectors: 0,
    }
}

fn new_temporal(
    device: &ProtocolObject<dyn MTLDevice>,
    info: &WMTFXTemporalScalerInfo,
) -> Retained<ProtocolObject<dyn MTLFXTemporalScaler>> {
    let mut p =
        UnixcallMtldeviceNewfxtemporalscaler { device: handle_of(device), info: mem(info), ret: 0 };
    assert_eq!(call(new_temporal_scaler, &mut p), 0);
    owned(p.ret)
}

#[test]
fn supports_answers_as_metalfx_does() {
    let device = device();
    let some = Some(&*device);
    for (slot, handle, device) in [("device", handle_of(&*device), some), ("nil", 0, None)] {
        let spatial = SpatialDescriptor::supports_device(device);
        let temporal = TemporalDescriptor::supports_device(device);
        let mut p = UnixcallGenericObjUint64Ret { handle, ret: u64::MAX };
        assert_eq!(call(supports_fx_spatial_scaler, &mut p), 0);
        assert_eq!(p.ret, u64::from(spatial), "spatial, {slot}");
        let mut p = UnixcallGenericObjUint64Ret { handle, ret: u64::MAX };
        assert_eq!(call(supports_fx_temporal_scaler, &mut p), 0);
        assert_eq!(p.ret, u64::from(temporal), "temporal, {slot}");
    }
}

#[test]
fn scale_range_answers_as_metalfx_does() {
    let device = device();
    for (handle, device) in [(handle_of(&*device), Some(&*device)), (0, None)] {
        let mut p = UnixcallMtldeviceFxtemporalscalerScale {
            device: handle,
            min_scale: -1.0,
            max_scale: -1.0,
        };
        assert_eq!(call(temporal_scaler_scale_range, &mut p), 0);
        same(p.min_scale, TemporalDescriptor::supported_input_content_min_scale(device));
        same(p.max_scale, TemporalDescriptor::supported_input_content_max_scale(device));
    }
}

#[test]
fn spatial_scaler_is_made_from_the_info() {
    let device = device();
    let info = WMTFXSpatialScalerInfo {
        color_format: RGBA16F,
        output_format: RGBA16F,
        input_width: 64,
        input_height: 48,
        output_width: 128,
        output_height: 96,
    };
    let mut p = UnixcallMtldeviceNewfxspatialscaler {
        device: handle_of(&*device),
        info: mem(&info),
        ret: 0,
    };
    assert_eq!(call(new_spatial_scaler, &mut p), 0);
    let scaler = owned::<ProtocolObject<dyn MTLFXSpatialScaler>>(p.ret);
    let sizes = [scaler.input_width(), scaler.input_height()];
    assert_eq!(sizes, [64, 48]);
    let sizes = [scaler.output_width(), scaler.output_height()];
    assert_eq!(sizes, [128, 96]);
}

#[test]
fn temporal_reactive_mask_format_needs_the_mask() {
    let device = device();
    let scaler = new_temporal(&device, &temporal_info(true));
    let sizes = [scaler.input_width(), scaler.input_height()];
    assert_eq!(sizes, [64, 48]);
    let sizes = [scaler.output_width(), scaler.output_height()];
    assert_eq!(sizes, [128, 96]);
    assert_eq!(scaler.color_texture_format(), MTLPixelFormat::RGBA16Float);
    assert_eq!(scaler.reactive_mask_texture_format(), MTLPixelFormat::R8Unorm);
    let scaler = new_temporal(&device, &temporal_info(false));
    assert_eq!(scaler.reactive_mask_texture_format(), MTLPixelFormat::Invalid);
}

#[test]
fn temporal_encode_sets_every_property() {
    let device = device();
    let scaler = new_temporal(&device, &temporal_info(true));
    let queue = device.newCommandQueue().expect("a queue");
    let cmdbuf = queue.commandBuffer().expect("a command buffer");
    let color = texture(&device, MTLPixelFormat::RGBA16Float, 64, scaler.color_texture_usage());
    let output = texture(&device, MTLPixelFormat::RGBA16Float, 128, scaler.output_texture_usage());
    let depth = texture(&device, MTLPixelFormat::Depth32Float, 64, scaler.depth_texture_usage());
    let motion = texture(&device, MTLPixelFormat::RG16Float, 64, scaler.motion_texture_usage());
    let exposure = texture(&device, MTLPixelFormat::R16Float, 1, MTLTextureUsage::ShaderRead);
    let usage = scaler.reactive_mask_texture_usage();
    let reactive = texture(&device, MTLPixelFormat::R8Unorm, 64, usage);
    let fence = device.newFence().expect("a fence");
    let props = WMTFXTemporalScalerProps {
        input_content_width: 48,
        input_content_height: 40,
        reset: 1,
        depth_reversed: 1,
        motion_vector_scale_x: 48.0,
        motion_vector_scale_y: -40.0,
        jitter_offset_x: 0.25,
        jitter_offset_y: -0.375,
        pre_exposure: 1.5,
        color_content_offset_x: 1,
        color_content_offset_y: 2,
        depth_content_offset_x: 3,
        depth_content_offset_y: 4,
        motion_content_offset_x: 5,
        motion_content_offset_y: 6,
        reactive_mask_content_offset_x: 7,
        reactive_mask_content_offset_y: 8,
        output_offset_x: 9,
        output_offset_y: 10,
    };
    let mut p = UnixcallMtlcommandbufferTemporalScale {
        cmdbuf: handle_of(&*cmdbuf),
        scaler: handle_of(&*scaler),
        color: handle_of(&*color),
        output: handle_of(&*output),
        depth: handle_of(&*depth),
        motion: handle_of(&*motion),
        exposure: handle_of(&*exposure),
        fence: handle_of(&*fence),
        props: mem(&props),
        reactive_mask: handle_of(&*reactive),
    };
    assert_eq!(call(encode_temporal_scale, &mut p), 0);
    cmdbuf.commit();
    cmdbuf.waitUntilCompleted();
    assert_eq!(cmdbuf.status(), MTLCommandBufferStatus::Completed);
    assert!(same_object(scaler.color_texture().as_deref(), &color));
    assert!(same_object(scaler.output_texture().as_deref(), &output));
    assert!(same_object(scaler.depth_texture().as_deref(), &depth));
    assert!(same_object(scaler.motion_texture().as_deref(), &motion));
    assert!(same_object(scaler.exposure_texture().as_deref(), &exposure));
    assert!(same_object(scaler.reactive_mask_texture().as_deref(), &reactive));
    assert!(same_object(scaler.fence().as_deref(), &fence));
    assert_eq!([scaler.input_content_width(), scaler.input_content_height()], [48, 40]);
    // `reset` is consumed by the encode: the probe pins its setter.
    assert!(scaler.depth_reversed());
    same(scaler.motion_vector_scale_x(), 48.0);
    same(scaler.motion_vector_scale_y(), -40.0);
    same(scaler.jitter_offset_x(), 0.25);
    same(scaler.jitter_offset_y(), -0.375);
    same(scaler.pre_exposure(), 1.5);
    assert_eq!(
        [
            scaler.color_content_offset_x(),
            scaler.color_content_offset_y(),
            scaler.depth_content_offset_x(),
            scaler.depth_content_offset_y(),
            scaler.motion_content_offset_x(),
            scaler.motion_content_offset_y(),
            scaler.reactive_mask_content_offset_x(),
            scaler.reactive_mask_content_offset_y(),
            scaler.output_offset_x(),
            scaler.output_offset_y(),
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
}

#[test]
fn spatial_encode_sets_the_textures() {
    let device = device();
    let info = WMTFXSpatialScalerInfo {
        color_format: RGBA16F,
        output_format: RGBA16F,
        input_width: 64,
        input_height: 64,
        output_width: 128,
        output_height: 128,
    };
    let mut made = UnixcallMtldeviceNewfxspatialscaler {
        device: handle_of(&*device),
        info: mem(&info),
        ret: 0,
    };
    assert_eq!(call(new_spatial_scaler, &mut made), 0);
    let scaler = owned::<ProtocolObject<dyn MTLFXSpatialScaler>>(made.ret);
    let queue = device.newCommandQueue().expect("a queue");
    let cmdbuf = queue.commandBuffer().expect("a command buffer");
    let color = texture(&device, MTLPixelFormat::RGBA16Float, 64, scaler.color_texture_usage());
    let output = texture(&device, MTLPixelFormat::RGBA16Float, 128, scaler.output_texture_usage());
    let fence = device.newFence().expect("a fence");
    let mut p = UnixcallMtlcommandbufferSpatialScale {
        cmdbuf: handle_of(&*cmdbuf),
        scaler: handle_of(&*scaler),
        color: handle_of(&*color),
        output: handle_of(&*output),
        fence: handle_of(&*fence),
    };
    assert_eq!(call(encode_spatial_scale, &mut p), 0);
    cmdbuf.commit();
    cmdbuf.waitUntilCompleted();
    assert_eq!(cmdbuf.status(), MTLCommandBufferStatus::Completed);
    assert!(same_object(scaler.color_texture().as_deref(), &color));
    assert!(same_object(scaler.output_texture().as_deref(), &output));
    assert!(same_object(scaler.fence().as_deref(), &fence));
}

fn interpolator_info(scaler: u64) -> WMTFXFrameInterpolatorInfo {
    WMTFXFrameInterpolatorInfo {
        color_format: RGBA16F,
        output_format: RGBA16F,
        depth_format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        motion_format: WMT_PIXEL_FORMAT_RG16_FLOAT,
        input_width: 128,
        input_height: 128,
        output_width: 128,
        output_height: 128,
        scaler,
    }
}

#[test]
fn interpolator_needs_a_supported_device() {
    let device = device();
    let info = interpolator_info(0);
    for (handle, supported) in [
        (handle_of(&*device), InterpolatorDescriptor::supports_device(Some(&device))),
        (0, InterpolatorDescriptor::supports_device(None)),
    ] {
        let mut p = UnixcallMtldeviceNewfxframeinterpolator {
            device: handle,
            info: mem(&info),
            ret: u64::MAX,
        };
        assert_eq!(call(new_frame_interpolator, &mut p), 0);
        assert_eq!(p.ret != 0, supported, "device {handle:#x}");
        if p.ret != 0 {
            drop(owned::<ProtocolObject<dyn MTLFXFrameInterpolator>>(p.ret));
        }
    }
}

#[test]
fn interpolate_encode_sets_every_property() {
    let device = device();
    let info = interpolator_info(0);
    let mut made = UnixcallMtldeviceNewfxframeinterpolator {
        device: handle_of(&*device),
        info: mem(&info),
        ret: u64::MAX,
    };
    assert_eq!(call(new_frame_interpolator, &mut made), 0);
    if made.ret == 0 {
        return;
    }
    let interpolator = owned::<ProtocolObject<dyn MTLFXFrameInterpolator>>(made.ret);
    let queue = device.newCommandQueue().expect("a queue");
    let cmdbuf = queue.commandBuffer().expect("a command buffer");
    let rgba = MTLPixelFormat::RGBA16Float;
    let color = texture(&device, rgba, 128, interpolator.color_texture_usage());
    let prev = texture(&device, rgba, 128, interpolator.color_texture_usage());
    let depth_usage = interpolator.depth_texture_usage();
    let depth = texture(&device, MTLPixelFormat::Depth32Float, 128, depth_usage);
    let motion_usage = interpolator.motion_texture_usage();
    let motion = texture(&device, MTLPixelFormat::RG16Float, 128, motion_usage);
    let output = texture(&device, rgba, 128, interpolator.output_texture_usage());
    let fence = device.newFence().expect("a fence");
    let props = WMTFXFrameInterpolatorProps {
        motion_vector_scale_x: 128.0,
        motion_vector_scale_y: -128.0,
        delta_time: 0.5,
        near_plane: 0.125,
        far_plane: 1000.0,
        field_of_view: 60.0,
        aspect_ratio: 1.5,
        jitter_offset_x: 0.25,
        jitter_offset_y: -0.375,
        reset: 1,
        depth_reversed: 1,
        ui_composited: 0,
    };
    let mut p = UnixcallMtlcommandbufferFrameInterpolate {
        cmdbuf: handle_of(&*cmdbuf),
        interpolator: handle_of(&*interpolator),
        color: handle_of(&*color),
        prev_color: handle_of(&*prev),
        depth: handle_of(&*depth),
        motion: handle_of(&*motion),
        output: handle_of(&*output),
        fence: handle_of(&*fence),
        props: mem(&props),
    };
    assert_eq!(call(encode_frame_interpolate, &mut p), 0);
    cmdbuf.commit();
    cmdbuf.waitUntilCompleted();
    assert_eq!(cmdbuf.status(), MTLCommandBufferStatus::Completed);
    assert!(same_object(interpolator.color_texture().as_deref(), &color));
    assert!(same_object(interpolator.prev_color_texture().as_deref(), &prev));
    assert!(same_object(interpolator.depth_texture().as_deref(), &depth));
    assert!(same_object(interpolator.motion_texture().as_deref(), &motion));
    assert!(same_object(interpolator.output_texture().as_deref(), &output));
    assert!(same_object(interpolator.fence().as_deref(), &fence));
    same(interpolator.motion_vector_scale_x(), 128.0);
    same(interpolator.motion_vector_scale_y(), -128.0);
    same(interpolator.delta_time(), 0.5);
    same(interpolator.near_plane(), 0.125);
    same(interpolator.far_plane(), 1000.0);
    same(interpolator.field_of_view(), 60.0);
    same(interpolator.aspect_ratio(), 1.5);
    same(interpolator.jitter_offset_x(), 0.25);
    same(interpolator.jitter_offset_y(), -0.375);
    // `shouldResetHistory` is consumed by the encode: the probe pins its setter.
    assert!(interpolator.depth_reversed());
    assert!(!interpolator.ui_texture_composited());
}

#[test]
fn a_nil_scaler_receives_nothing() {
    let props = WMTFXTemporalScalerProps {
        input_content_width: 1,
        input_content_height: 1,
        reset: 0,
        depth_reversed: 0,
        motion_vector_scale_x: 0.0,
        motion_vector_scale_y: 0.0,
        jitter_offset_x: 0.0,
        jitter_offset_y: 0.0,
        pre_exposure: 1.0,
        color_content_offset_x: 0,
        color_content_offset_y: 0,
        depth_content_offset_x: 0,
        depth_content_offset_y: 0,
        motion_content_offset_x: 0,
        motion_content_offset_y: 0,
        reactive_mask_content_offset_x: 0,
        reactive_mask_content_offset_y: 0,
        output_offset_x: 0,
        output_offset_y: 0,
    };
    let mut temporal = UnixcallMtlcommandbufferTemporalScale {
        cmdbuf: 0,
        scaler: 0,
        color: 0,
        output: 0,
        depth: 0,
        motion: 0,
        exposure: 0,
        fence: 0,
        props: mem(&props),
        reactive_mask: 0,
    };
    assert_eq!(call(encode_temporal_scale, &mut temporal), 0);
    let mut spatial = UnixcallMtlcommandbufferSpatialScale {
        cmdbuf: 0,
        scaler: 0,
        color: 0,
        output: 0,
        fence: 0,
    };
    assert_eq!(call(encode_spatial_scale, &mut spatial), 0);
    let mut interpolate = UnixcallMtlcommandbufferFrameInterpolate {
        cmdbuf: 0,
        interpolator: 0,
        color: 0,
        prev_color: 0,
        depth: 0,
        motion: 0,
        output: 0,
        fence: 0,
        props: MemPtr { ptr: 0 },
    };
    assert_eq!(call(encode_frame_interpolate, &mut interpolate), 0);
}

#[test]
fn a_null_info_makes_nothing() {
    let null = MemPtr { ptr: 0 };
    let mut temporal =
        UnixcallMtldeviceNewfxtemporalscaler { device: 0, info: null, ret: u64::MAX };
    assert_eq!(call(new_temporal_scaler, &mut temporal), 0);
    assert_eq!(temporal.ret, u64::MAX);
    let mut spatial = UnixcallMtldeviceNewfxspatialscaler { device: 0, info: null, ret: u64::MAX };
    assert_eq!(call(new_spatial_scaler, &mut spatial), 0);
    assert_eq!(spatial.ret, u64::MAX);
    let mut interpolator =
        UnixcallMtldeviceNewfxframeinterpolator { device: 0, info: null, ret: u64::MAX };
    assert_eq!(call(new_frame_interpolator, &mut interpolator), 0);
    assert_eq!(interpolator.ret, 0);
}
