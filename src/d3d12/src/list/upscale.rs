//! `IMTLD3D12CommandListExt::TemporalUpscale` (`d3d12_command_list.cpp:2382`).

use core::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
    sync::atomic::Ordering,
};
use std::sync::Arc;

use dxmt_core::{
    format::{self, MtlDxgiFormatDesc},
    presenter::framegen::{self, CameraParams, FrameGenUi},
    resource::{Texture, TextureAllocationFlags, TextureViewKey},
};
use dxmt_util::{
    com::Guid,
    dxgi_consts::{
        DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8B8A8_TYPELESS, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R10G10B10A2_TYPELESS, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16_TYPELESS,
        DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_TYPELESS,
        DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32B32A32_TYPELESS,
    },
};
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_PIXEL_FORMAT_INVALID, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
    WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE,
    WMTFXTemporalDenoisedScalerProps, WMTFXTemporalScalerProps, WMTOrigin, WMTSize, WMTTextureInfo,
    original_format,
};

use crate::{
    abi::ext::{D3D12DenoisedUpscaleDesc, D3D12FrameGenUiDesc, D3D12UpscaleDesc},
    allocator::Allocator,
    com::{E_INVALIDARG, Hresult, S_OK},
    device::{
        Device,
        temporal::{DenoisedScratch, Scratch, TemporalScalerObject, cast_temporal_scaler},
    },
    encoder::{
        EncoderData, EncoderType, TemporalDenoisedUpscaleEncoderData, TemporalUpscaleEncoderData,
        TextureViewRef,
    },
    list::{List, copy},
    resource::Resource,
};

const WRITABLE_USAGE: u32 = WMT_TEXTURE_USAGE_SHADER_READ
    | WMT_TEXTURE_USAGE_SHADER_WRITE
    | WMT_TEXTURE_USAGE_RENDER_TARGET;

const DLSSG_CAMERA_GUID: Guid = Guid::new(
    0x7a3f_91c4,
    0x5e82,
    0x4b1d,
    [0x93, 0xa7, 0x2c, 0x41, 0x8f, 0xd0, 0x62, 0x19],
);

const fn as_format_desc(pixel_format: u32, bytes_per_texel: u32) -> MtlDxgiFormatDesc {
    MtlDxgiFormatDesc {
        pixel_format,
        attribute_format: 0,
        bytes_per_texel,
        planar_count: 0,
        flag: 0,
    }
}

const fn reactive_format_compatible(res_format: u32, scaler_format: u32) -> bool {
    if res_format == scaler_format {
        return true;
    }
    match res_format {
        DXGI_FORMAT_R8_TYPELESS => scaler_format == DXGI_FORMAT_R8_UNORM,
        DXGI_FORMAT_R16_TYPELESS => scaler_format == DXGI_FORMAT_R16_UNORM,
        DXGI_FORMAT_R8G8B8A8_TYPELESS => scaler_format == DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_TYPELESS => scaler_format == DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_R10G10B10A2_TYPELESS => scaler_format == DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R16G16B16A16_TYPELESS => scaler_format == DXGI_FORMAT_R16G16B16A16_FLOAT,
        DXGI_FORMAT_R32G32B32A32_TYPELESS => scaler_format == DXGI_FORMAT_R32G32B32A32_FLOAT,
        _ => false,
    }
}

fn take_texture_matching(
    resource: *mut c_void,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    format_ok: impl FnOnce(u32) -> bool,
) -> Option<Arc<Texture>> {
    if resource.is_null() {
        return None;
    }
    // SAFETY: `resource` is a caller-supplied `ID3D12Resource` pointer.
    let res_obj = unsafe { &*resource.cast::<Resource>() };
    let tex = res_obj.texture.as_ref()?;
    if tex.info().r#type() != WMT_TEXTURE_TYPE2D
        || !format_ok(res_obj.desc_scoped().format)
        || u64::from(x) + u64::from(w) > u64::from(tex.info().width)
        || u64::from(y) + u64::from(h) > u64::from(tex.info().height)
    {
        return None;
    }
    Some(Arc::clone(tex))
}

fn take_texture(
    resource: *mut c_void,
    format: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<Arc<Texture>> {
    take_texture_matching(resource, x, y, w, h, |res_fmt| res_fmt == format)
}

fn take_reactive_texture(
    resource: *mut c_void,
    format: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<Arc<Texture>> {
    take_texture_matching(resource, x, y, w, h, |res_fmt| {
        reactive_format_compatible(res_fmt, format)
    })
}

const fn readable(t: &Texture, format: u32) -> bool {
    t.info().pixel_format == format && (t.info().usage() & WMT_TEXTURE_USAGE_SHADER_READ) != 0
}

const fn same_layout(t: &Texture, format: u32) -> bool {
    let pix = t.info().pixel_format;
    if pix == format {
        return true;
    }
    let own = format::unsigned_integer(pix);
    own != WMT_PIXEL_FORMAT_INVALID && own == format::unsigned_integer(format)
}

const fn can_view_directly(t: &Texture, format: u32) -> bool {
    same_layout(t, format) && (t.info().usage() & WMT_TEXTURE_USAGE_SHADER_READ) != 0
}

fn allocate_reactive_scratch(
    device: *mut Device,
    width: u32,
    height: u32,
    pixel_format: u32,
) -> Option<Arc<Texture>> {
    if device.is_null() {
        return None;
    }
    // SAFETY: `device` is a live `Device` pointer owning `mtl`.
    let mtl = unsafe { (*device).mtl.as_ref() }?;
    let usage = WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    let bits = (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
        | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        | (usage << WMTTextureInfo::USAGE_SHIFT);
    let info = WMTTextureInfo {
        pixel_format: original_format(pixel_format),
        width,
        height,
        depth: 1,
        array_length: 1,
        bits,
        options: WMT_RESOURCE_STORAGE_MODE_PRIVATE | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let scratch = Arc::new(Texture::new(info, mtl));
    let alloc = scratch.allocate(TextureAllocationFlags::GPU_PRIVATE);
    let _ = scratch.rename(alloc);
    Some(scratch)
}

fn read_camera_params(list: &List) -> CameraParams {
    #[repr(C)]
    struct RawCamera {
        near_plane: f32,
        far_plane: f32,
        field_of_view: f32,
        aspect_ratio: f32,
    }
    let mut raw = RawCamera {
        near_plane: 0.0,
        far_plane: 0.0,
        field_of_view: 0.0,
        aspect_ratio: 0.0,
    };
    let expected = u32::try_from(size_of::<RawCamera>()).unwrap_or(16);
    let mut size = expected;
    // SAFETY: `raw` is a local `RawCamera` of `size` writable bytes.
    let hr = unsafe {
        list.private_data
            .get_data(DLSSG_CAMERA_GUID, &raw mut size, (&raw mut raw).cast())
    };
    if hr >= 0 && size == expected {
        CameraParams {
            near_plane: raw.near_plane,
            far_plane: raw.far_plane,
            field_of_view: raw.field_of_view,
            aspect_ratio: raw.aspect_ratio,
        }
    } else {
        CameraParams::ZERO
    }
}

fn check_view_use_format(texture: &Texture, format: u32) -> TextureViewKey {
    let view = texture.full_view();
    if texture.info().pixel_format == format {
        view
    } else {
        view.with_format(format)
    }
}

fn consume_reset(desc_reset: i32, fresh: &core::sync::atomic::AtomicBool) -> u8 {
    u8::from(desc_reset != 0 || fresh.swap(false, Ordering::SeqCst))
}

/// `IMTLD3D12CommandListExt::TemporalUpscale` (`d3d12_command_list.cpp:2382`).
///
/// # Safety
///
/// `this` is the `IMTLD3D12CommandListExt` companion pointer at `List::EXT_OFFSET`; `scaler` and
/// `desc` are caller-supplied pointers.
pub unsafe fn temporal_upscale(
    this: *mut c_void,
    scaler: *mut c_void,
    desc: *const D3D12UpscaleDesc,
) -> Hresult {
    crate::scope!("list.TemporalUpscale");
    if this.is_null() || scaler.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` points to `ext_vtbl` inside `List` at `List::EXT_OFFSET`.
    let base_ptr = unsafe { this.byte_sub(List::EXT_OFFSET).cast::<List>() };
    // SAFETY: `base_ptr` is the enclosing `List` allocation.
    let list = unsafe { &mut *base_ptr };
    if list.allocator.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `scaler` is non-null; `cast_temporal_scaler` checks its vtable before dereferencing.
    let Some(scaler) = (unsafe { cast_temporal_scaler(scaler) }) else {
        return E_INVALIDARG;
    };
    if scaler.denoised {
        return E_INVALIDARG;
    }
    let sd = &scaler.desc;
    let info = &scaler.info;
    // SAFETY: `desc` was checked non-null above.
    let d = unsafe { &*desc };

    let (mw, mh) = if sd.output_resolution_motion_vectors != 0 {
        (sd.output_width, sd.output_height)
    } else {
        (sd.input_width, sd.input_height)
    };
    let masked = info.reactive_mask_enabled != 0;

    let color = take_texture(
        d.color,
        sd.color_format,
        d.color_offset[0],
        d.color_offset[1],
        sd.input_width,
        sd.input_height,
    );
    let depth = take_texture(
        d.depth,
        sd.depth_format,
        d.depth_offset[0],
        d.depth_offset[1],
        sd.input_width,
        sd.input_height,
    );
    let motion = take_texture(
        d.motion_vector,
        sd.motion_format,
        d.motion_offset[0],
        d.motion_offset[1],
        mw,
        mh,
    );
    let output = take_texture(
        d.output,
        sd.output_format,
        d.output_offset[0],
        d.output_offset[1],
        sd.output_width,
        sd.output_height,
    );
    let mut mask = if masked {
        take_reactive_texture(
            d.reactive_mask,
            sd.reactive_mask_format,
            d.reactive_offset[0],
            d.reactive_offset[1],
            sd.input_width,
            sd.input_height,
        )
    } else {
        None
    };
    let exposure_tex = if d.exposure.is_null() {
        None
    } else {
        // SAFETY: `d.exposure` is a caller-supplied `ID3D12Resource` pointer.
        let exp_res = unsafe { &*d.exposure.cast::<Resource>() };
        let Some(tex) = exp_res.texture.as_ref() else {
            return E_INVALIDARG;
        };
        Some(Arc::clone(tex))
    };

    let (Some(color), Some(mut depth), Some(mut motion), Some(output)) =
        (color, depth, motion, output)
    else {
        return E_INVALIDARG;
    };

    if (masked && mask.is_none())
        || (!masked && !d.reactive_mask.is_null())
        || !same_layout(&color, info.color_format)
        || (color.info().usage() & WMT_TEXTURE_USAGE_SHADER_READ) == 0
        || !same_layout(&output, info.output_format)
    {
        return E_INVALIDARG;
    }

    if d.input_content_width == 0
        || d.input_content_height == 0
        || d.input_content_width > sd.input_width
        || d.input_content_height > sd.input_height
    {
        return E_INVALIDARG;
    }
    let sx = f64::from(sd.output_width) / f64::from(d.input_content_width);
    let sy = f64::from(sd.output_height) / f64::from(d.input_content_height);
    let min_scale = f64::from(info.input_content_min_scale);
    let max_scale = f64::from(info.input_content_max_scale);
    if sx < min_scale || sy < min_scale || sx > max_scale || sy > max_scale {
        return E_INVALIDARG;
    }

    let mut props = WMTFXTemporalScalerProps {
        input_content_width: d.input_content_width,
        input_content_height: d.input_content_height,
        reset: consume_reset(d.reset, &scaler.fresh),
        depth_reversed: u8::from(d.depth_reversed != 0),
        motion_vector_scale_x: d.motion_vector_scale[0],
        motion_vector_scale_y: d.motion_vector_scale[1],
        jitter_offset_x: d.jitter_offset[0],
        jitter_offset_y: d.jitter_offset[1],
        pre_exposure: d.pre_exposure,
        color_content_offset_x: d.color_offset[0],
        color_content_offset_y: d.color_offset[1],
        depth_content_offset_x: d.depth_offset[0],
        depth_content_offset_y: d.depth_offset[1],
        motion_content_offset_x: d.motion_offset[0],
        motion_content_offset_y: d.motion_offset[1],
        reactive_mask_content_offset_x: d.reactive_offset[0],
        reactive_mask_content_offset_y: d.reactive_offset[1],
        output_offset_x: d.output_offset[0],
        output_offset_y: d.output_offset[1],
    };

    let reactive_scratch = if let Some(ref m) = mask
        && !can_view_directly(m, info.reactive_mask_format)
    {
        if format::texel_size(m.info().pixel_format)
            != format::texel_size(info.reactive_mask_format)
        {
            return E_INVALIDARG;
        }
        let Some(scratch) = allocate_reactive_scratch(
            list.device,
            sd.input_width,
            sd.input_height,
            info.reactive_mask_format,
        ) else {
            return E_INVALIDARG;
        };
        Some(scratch)
    } else {
        None
    };

    framegen::set_camera_params(&read_camera_params(list));

    let mut depth_scratch: Option<Arc<Texture>> = None;
    let mut motion_scratch: Option<Arc<Texture>> = None;
    let mut output_scratch: Option<Arc<Texture>> = None;

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !readable(&depth, info.depth_format)
        || !readable(&motion, info.motion_format)
        || reactive_scratch.is_some()
    {
        crate::count!("#upscale input copied", 1);
        alloc.invalidate_current_pass();
        copy::pre_blit(alloc, ptr::null(), false);
        if !readable(&depth, info.depth_format)
            && let Some(scratch) = scaler.get_scratch(Scratch::Depth)
        {
            alloc.writes(Arc::as_ptr(&scratch).cast::<c_void>());
            let src_fmt = as_format_desc(depth.info().pixel_format, 4);
            let dst_fmt = as_format_desc(info.depth_format, 4);
            copy::copy_reinterpret(
                alloc,
                &copy::CopyReinterpretSide {
                    texture: &depth,
                    format: &src_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin {
                        x: u64::from(d.depth_offset[0]),
                        y: u64::from(d.depth_offset[1]),
                        z: 0,
                    },
                },
                WMTSize {
                    width: u64::from(sd.input_width),
                    height: u64::from(sd.input_height),
                    depth: 1,
                },
                &copy::CopyReinterpretSide {
                    texture: &scratch,
                    format: &dst_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin { x: 0, y: 0, z: 0 },
                },
            );
            props.depth_content_offset_x = 0;
            props.depth_content_offset_y = 0;
            depth = Arc::clone(&scratch);
            depth_scratch = Some(scratch);
        }
        if !readable(&motion, info.motion_format)
            && let Some(scratch) = scaler.get_scratch(Scratch::Motion)
        {
            alloc.writes(Arc::as_ptr(&scratch).cast::<c_void>());
            let src_fmt = as_format_desc(
                motion.info().pixel_format,
                format::texel_size(motion.info().pixel_format),
            );
            let dst_fmt =
                as_format_desc(info.motion_format, format::texel_size(info.motion_format));
            copy::copy_reinterpret(
                alloc,
                &copy::CopyReinterpretSide {
                    texture: &motion,
                    format: &src_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin {
                        x: u64::from(d.motion_offset[0]),
                        y: u64::from(d.motion_offset[1]),
                        z: 0,
                    },
                },
                WMTSize {
                    width: u64::from(mw),
                    height: u64::from(mh),
                    depth: 1,
                },
                &copy::CopyReinterpretSide {
                    texture: &scratch,
                    format: &dst_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin { x: 0, y: 0, z: 0 },
                },
            );
            props.motion_content_offset_x = 0;
            props.motion_content_offset_y = 0;
            motion = Arc::clone(&scratch);
            motion_scratch = Some(scratch);
        }
        if let Some(scratch) = reactive_scratch
            && let Some(ref m) = mask
        {
            alloc.writes(Arc::as_ptr(&scratch).cast::<c_void>());
            let src_fmt = as_format_desc(
                m.info().pixel_format,
                format::texel_size(m.info().pixel_format),
            );
            let dst_fmt = as_format_desc(
                info.reactive_mask_format,
                format::texel_size(info.reactive_mask_format),
            );
            copy::copy_reinterpret(
                alloc,
                &copy::CopyReinterpretSide {
                    texture: m,
                    format: &src_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin {
                        x: u64::from(d.reactive_offset[0]),
                        y: u64::from(d.reactive_offset[1]),
                        z: 0,
                    },
                },
                WMTSize {
                    width: u64::from(sd.input_width),
                    height: u64::from(sd.input_height),
                    depth: 1,
                },
                &copy::CopyReinterpretSide {
                    texture: &scratch,
                    format: &dst_fmt,
                    level: 0,
                    slice: 0,
                    origin: WMTOrigin { x: 0, y: 0, z: 0 },
                },
            );
            props.reactive_mask_content_offset_x = 0;
            props.reactive_mask_content_offset_y = 0;
            alloc.keep_texture(Arc::clone(&scratch));
            mask = Some(scratch);
        }
    }

    let mut target = Arc::clone(&output);
    let output_private = output
        .current()
        .is_some_and(|a| a.flags().contains(TextureAllocationFlags::GPU_PRIVATE));
    if (output.info().usage() & WRITABLE_USAGE) != WRITABLE_USAGE || !output_private {
        crate::count!("#upscale output copied", 1);
        if let Some(scratch) = scaler.get_scratch(Scratch::Output) {
            props.output_offset_x = 0;
            props.output_offset_y = 0;
            target = Arc::clone(&scratch);
            output_scratch = Some(scratch);
        }
    }

    alloc.invalidate_current_pass();
    let barriers = alloc.barriers;
    let ptr = alloc
        .allocate_cpu_heap(
            size_of::<TemporalUpscaleEncoderData>(),
            align_of::<TemporalUpscaleEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<TemporalUpscaleEncoderData>();
    let mut base = EncoderData::new(EncoderType::TemporalUpscale, 0);
    base.barriers = barriers;
    base.barriers_last = barriers;
    let init = TemporalUpscaleEncoderData {
        base,
        scaler: Some(Arc::clone(&scaler.scaler)),
        color: TextureViewRef::from_texture(
            &color,
            check_view_use_format(&color, info.color_format),
        ),
        depth: TextureViewRef::from_texture(&depth, depth.full_view()),
        motion: TextureViewRef::from_texture(&motion, motion.full_view()),
        exposure: exposure_tex
            .as_ref()
            .and_then(|t| TextureViewRef::from_texture(t, t.full_view())),
        reactive: mask.as_ref().and_then(|t| {
            TextureViewRef::from_texture(t, check_view_use_format(t, info.reactive_mask_format))
        }),
        output: TextureViewRef::from_texture(
            &target,
            check_view_use_format(&target, info.output_format),
        ),
        depth_scratch,
        motion_scratch,
        output_scratch: output_scratch.clone(),
        props,
    };
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
    // `TemporalUpscaleEncoderData`.
    unsafe { ptr::write(ptr, init) };
    alloc.encoder_current = ptr.cast::<EncoderData>();
    alloc.invalidate_current_pass();

    if let Some(scratch) = output_scratch {
        // SAFETY: `d.output` is a valid non-null `Resource` pointer.
        let dst_key = unsafe { copy::resource_key(d.output) };
        copy::pre_blit(alloc, dst_key, false);
        let bytes = format::texel_size(output.info().pixel_format);
        let src_fmt = as_format_desc(info.output_format, bytes);
        let dst_fmt = as_format_desc(output.info().pixel_format, bytes);
        copy::copy_reinterpret(
            alloc,
            &copy::CopyReinterpretSide {
                texture: &scratch,
                format: &src_fmt,
                level: 0,
                slice: 0,
                origin: WMTOrigin { x: 0, y: 0, z: 0 },
            },
            WMTSize {
                width: u64::from(sd.output_width),
                height: u64::from(sd.output_height),
                depth: 1,
            },
            &copy::CopyReinterpretSide {
                texture: &output,
                format: &dst_fmt,
                level: 0,
                slice: 0,
                origin: WMTOrigin {
                    x: u64::from(d.output_offset[0]),
                    y: u64::from(d.output_offset[1]),
                    z: 0,
                },
            },
        );
    }

    S_OK
}

fn copy_input_scratch(
    alloc: &mut Allocator,
    pre_blitted: &mut bool,
    src: &Arc<Texture>,
    dst: &Arc<Texture>,
    dst_format: u32,
    w: u32,
    h: u32,
) {
    if !*pre_blitted {
        crate::count!("#upscale input copied", 1);
        alloc.invalidate_current_pass();
        copy::pre_blit(alloc, ptr::null(), false);
        *pre_blitted = true;
    }
    alloc.writes(Arc::as_ptr(dst).cast::<c_void>());
    let src_pix = src.info().pixel_format;
    let src_bytes = format::texel_size(src_pix);
    let dst_bytes = format::texel_size(dst_format);
    let src_fmt = as_format_desc(src_pix, if src_bytes == 0 { 4 } else { src_bytes });
    let dst_fmt = as_format_desc(dst_format, if dst_bytes == 0 { 4 } else { dst_bytes });
    copy::copy_reinterpret(
        alloc,
        &copy::CopyReinterpretSide {
            texture: src,
            format: &src_fmt,
            level: 0,
            slice: 0,
            origin: WMTOrigin { x: 0, y: 0, z: 0 },
        },
        WMTSize {
            width: u64::from(w),
            height: u64::from(h),
            depth: 1,
        },
        &copy::CopyReinterpretSide {
            texture: dst,
            format: &dst_fmt,
            level: 0,
            slice: 0,
            origin: WMTOrigin { x: 0, y: 0, z: 0 },
        },
    );
}

fn prepare_denoised_channel(
    alloc: &mut Allocator,
    pre_blitted: &mut bool,
    scaler: &TemporalScalerObject,
    channel: DenoisedScratch,
    tex: &mut Arc<Texture>,
    target_format: u32,
    dims: (u32, u32),
) -> Result<Option<Arc<Texture>>, Hresult> {
    if can_view_directly(tex, target_format) {
        return Ok(None);
    }
    if format::texel_size(tex.info().pixel_format) != format::texel_size(target_format) {
        return Err(E_INVALIDARG);
    }
    let Some(scratch) = scaler.get_denoised_scratch(channel, target_format) else {
        return Err(E_INVALIDARG);
    };
    copy_input_scratch(
        alloc,
        pre_blitted,
        tex,
        &scratch,
        target_format,
        dims.0,
        dims.1,
    );
    *tex = Arc::clone(&scratch);
    Ok(Some(scratch))
}

fn take_optional_denoised(
    resource: *mut c_void,
    enabled: bool,
    dxgi_format: u32,
    w: u32,
    h: u32,
) -> Result<Option<Arc<Texture>>, Hresult> {
    if !enabled {
        return if resource.is_null() {
            Ok(None)
        } else {
            Err(E_INVALIDARG)
        };
    }
    if resource.is_null() {
        return Err(E_INVALIDARG);
    }
    take_reactive_texture(resource, dxgi_format, 0, 0, w, h)
        .map(Some)
        .ok_or(E_INVALIDARG)
}

fn resolve_exposure(resource: *mut c_void) -> Result<Option<Arc<Texture>>, Hresult> {
    if resource.is_null() {
        return Ok(None);
    }
    // SAFETY: `resource` is a caller-supplied `ID3D12Resource` pointer.
    let exp_res = unsafe { &*resource.cast::<Resource>() };
    let Some(tex) = exp_res.texture.as_ref() else {
        return Err(E_INVALIDARG);
    };
    Ok(Some(Arc::clone(tex)))
}

fn view_for_format(tex: Option<&Arc<Texture>>, format: u32) -> Option<TextureViewRef> {
    tex.and_then(|t| TextureViewRef::from_texture(t, check_view_use_format(t, format)))
}

/// `IMTLD3D12CommandListExt1::TemporalDenoisedUpscale`.
///
/// # Safety
///
/// `this` is the `IMTLD3D12CommandListExt1` companion pointer at `List::EXT_OFFSET`; `scaler` and
/// `desc` are caller-supplied pointers.
pub unsafe fn temporal_denoised_upscale(
    this: *mut c_void,
    scaler: *mut c_void,
    desc: *const D3D12DenoisedUpscaleDesc,
) -> Hresult {
    crate::scope!("list.TemporalDenoisedUpscale");
    if this.is_null() || scaler.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` points to `ext_vtbl` inside `List` at `List::EXT_OFFSET`.
    let base_ptr = unsafe { this.byte_sub(List::EXT_OFFSET).cast::<List>() };
    // SAFETY: `base_ptr` is the enclosing `List` allocation.
    let list = unsafe { &mut *base_ptr };
    if list.allocator.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `scaler` is non-null; `cast_temporal_scaler` checks its vtable before dereferencing.
    let Some(scaler) = (unsafe { cast_temporal_scaler(scaler) }) else {
        return E_INVALIDARG;
    };
    if !scaler.denoised {
        return E_INVALIDARG;
    }
    let sd = &scaler.denoised_desc;
    let info = &scaler.denoised_info;
    // SAFETY: `desc` was checked non-null above.
    let d = unsafe { desc.read_unaligned() };

    if d.input_content_width == 0
        || d.input_content_height == 0
        || d.input_content_width > sd.input_width
        || d.input_content_height > sd.input_height
    {
        return E_INVALIDARG;
    }

    let (iw, ih) = (sd.input_width, sd.input_height);
    let color = take_reactive_texture(d.color, sd.color_format, 0, 0, iw, ih);
    let output = take_reactive_texture(
        d.output,
        sd.output_format,
        0,
        0,
        sd.output_width,
        sd.output_height,
    );
    let depth = take_reactive_texture(d.depth, sd.depth_format, 0, 0, iw, ih);
    let motion = take_reactive_texture(d.motion_vector, sd.motion_format, 0, 0, iw, ih);
    let diffuse_albedo =
        take_reactive_texture(d.diffuse_albedo, sd.diffuse_albedo_format, 0, 0, iw, ih);
    let specular_albedo =
        take_reactive_texture(d.specular_albedo, sd.specular_albedo_format, 0, 0, iw, ih);
    let normal = take_reactive_texture(d.normal, sd.normal_format, 0, 0, iw, ih);
    let roughness = take_reactive_texture(d.roughness, sd.roughness_format, 0, 0, iw, ih);

    let (
        Some(color),
        Some(output),
        Some(mut depth),
        Some(mut motion),
        Some(mut diffuse_albedo),
        Some(mut specular_albedo),
        Some(mut normal),
        Some(mut roughness),
    ) = (
        color,
        output,
        depth,
        motion,
        diffuse_albedo,
        specular_albedo,
        normal,
        roughness,
    )
    else {
        return E_INVALIDARG;
    };

    if !same_layout(&color, info.color_format)
        || (color.info().usage() & WMT_TEXTURE_USAGE_SHADER_READ) == 0
        || !same_layout(&output, info.output_format)
    {
        return E_INVALIDARG;
    }

    let Ok(mut specular_hit_distance) = take_optional_denoised(
        d.specular_hit_distance,
        info.specular_hit_distance_enabled != 0,
        sd.specular_hit_distance_format,
        iw,
        ih,
    ) else {
        return E_INVALIDARG;
    };
    let Ok(mut denoise_strength_mask) = take_optional_denoised(
        d.denoise_strength_mask,
        info.denoise_strength_mask_enabled != 0,
        sd.denoise_strength_mask_format,
        iw,
        ih,
    ) else {
        return E_INVALIDARG;
    };
    let Ok(mut transparency_overlay) = take_optional_denoised(
        d.transparency_overlay,
        info.transparency_overlay_enabled != 0,
        sd.transparency_overlay_format,
        iw,
        ih,
    ) else {
        return E_INVALIDARG;
    };
    let Ok(exposure_tex) = resolve_exposure(d.exposure) else {
        return E_INVALIDARG;
    };
    let Ok(mut mask) = take_optional_denoised(
        d.reactive_mask,
        info.reactive_mask_enabled != 0,
        sd.reactive_mask_format,
        iw,
        ih,
    ) else {
        return E_INVALIDARG;
    };

    let props = WMTFXTemporalDenoisedScalerProps {
        pre_exposure: d.pre_exposure,
        jitter_offset_x: d.jitter_offset[0],
        jitter_offset_y: d.jitter_offset[1],
        motion_vector_scale_x: d.motion_vector_scale[0],
        motion_vector_scale_y: d.motion_vector_scale[1],
        input_content_width: d.input_content_width,
        input_content_height: d.input_content_height,
        reset: consume_reset(d.reset, &scaler.fresh),
        depth_reversed: u8::from(d.depth_reversed != 0),
        has_matrices: u8::from(d.has_matrices != 0),
        pad: 0,
        world_to_view_matrix: d.world_to_view_matrix,
        view_to_clip_matrix: d.view_to_clip_matrix,
    };

    framegen::set_camera_params(&read_camera_params(list));

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let mut pre_blitted = false;
    let mut depth_scratch = None;
    let mut motion_scratch = None;

    if !readable(&depth, info.depth_format)
        && let Some(scratch) = scaler.get_scratch(Scratch::Depth)
    {
        copy_input_scratch(
            alloc,
            &mut pre_blitted,
            &depth,
            &scratch,
            info.depth_format,
            iw,
            ih,
        );
        depth = Arc::clone(&scratch);
        depth_scratch = Some(scratch);
    }
    if !readable(&motion, info.motion_format)
        && let Some(scratch) = scaler.get_scratch(Scratch::Motion)
    {
        copy_input_scratch(
            alloc,
            &mut pre_blitted,
            &motion,
            &scratch,
            info.motion_format,
            iw,
            ih,
        );
        motion = Arc::clone(&scratch);
        motion_scratch = Some(scratch);
    }

    let Ok(diffuse_albedo_scratch) = prepare_denoised_channel(
        alloc,
        &mut pre_blitted,
        scaler,
        DenoisedScratch::DiffuseAlbedo,
        &mut diffuse_albedo,
        info.diffuse_albedo_format,
        (iw, ih),
    ) else {
        return E_INVALIDARG;
    };
    let Ok(specular_albedo_scratch) = prepare_denoised_channel(
        alloc,
        &mut pre_blitted,
        scaler,
        DenoisedScratch::SpecularAlbedo,
        &mut specular_albedo,
        info.specular_albedo_format,
        (iw, ih),
    ) else {
        return E_INVALIDARG;
    };
    let Ok(normal_scratch) = prepare_denoised_channel(
        alloc,
        &mut pre_blitted,
        scaler,
        DenoisedScratch::Normal,
        &mut normal,
        info.normal_format,
        (iw, ih),
    ) else {
        return E_INVALIDARG;
    };
    let Ok(roughness_scratch) = prepare_denoised_channel(
        alloc,
        &mut pre_blitted,
        scaler,
        DenoisedScratch::Roughness,
        &mut roughness,
        info.roughness_format,
        (iw, ih),
    ) else {
        return E_INVALIDARG;
    };

    let specular_hit_distance_scratch = if let Some(ref mut tex) = specular_hit_distance {
        match prepare_denoised_channel(
            alloc,
            &mut pre_blitted,
            scaler,
            DenoisedScratch::SpecularHitDistance,
            tex,
            info.specular_hit_distance_format,
            (iw, ih),
        ) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    } else {
        None
    };
    let denoise_strength_mask_scratch = if let Some(ref mut tex) = denoise_strength_mask {
        match prepare_denoised_channel(
            alloc,
            &mut pre_blitted,
            scaler,
            DenoisedScratch::DenoiseStrengthMask,
            tex,
            info.denoise_strength_mask_format,
            (iw, ih),
        ) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    } else {
        None
    };
    let transparency_overlay_scratch = if let Some(ref mut tex) = transparency_overlay {
        match prepare_denoised_channel(
            alloc,
            &mut pre_blitted,
            scaler,
            DenoisedScratch::TransparencyOverlay,
            tex,
            info.transparency_overlay_format,
            (iw, ih),
        ) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    } else {
        None
    };
    let reactive_scratch = if let Some(ref mut tex) = mask {
        match prepare_denoised_channel(
            alloc,
            &mut pre_blitted,
            scaler,
            DenoisedScratch::ReactiveMask,
            tex,
            info.reactive_mask_format,
            (iw, ih),
        ) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    } else {
        None
    };

    let mut output_scratch: Option<Arc<Texture>> = None;
    let mut target = Arc::clone(&output);
    let output_private = output
        .current()
        .is_some_and(|a| a.flags().contains(TextureAllocationFlags::GPU_PRIVATE));
    if (output.info().usage() & WRITABLE_USAGE) != WRITABLE_USAGE || !output_private {
        crate::count!("#upscale output copied", 1);
        if let Some(scratch) = scaler.get_scratch(Scratch::Output) {
            target = Arc::clone(&scratch);
            output_scratch = Some(scratch);
        }
    }

    alloc.invalidate_current_pass();
    let barriers = alloc.barriers;
    let ptr = alloc
        .allocate_cpu_heap(
            size_of::<TemporalDenoisedUpscaleEncoderData>(),
            align_of::<TemporalDenoisedUpscaleEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<TemporalDenoisedUpscaleEncoderData>();
    let mut base = EncoderData::new(EncoderType::TemporalDenoisedScale, 0);
    base.barriers = barriers;
    base.barriers_last = barriers;
    let init = TemporalDenoisedUpscaleEncoderData {
        base,
        scaler: Some(Arc::clone(&scaler.scaler)),
        color: TextureViewRef::from_texture(
            &color,
            check_view_use_format(&color, info.color_format),
        ),
        output: TextureViewRef::from_texture(
            &target,
            check_view_use_format(&target, info.output_format),
        ),
        depth: TextureViewRef::from_texture(&depth, depth.full_view()),
        motion: TextureViewRef::from_texture(&motion, motion.full_view()),
        diffuse_albedo: TextureViewRef::from_texture(
            &diffuse_albedo,
            check_view_use_format(&diffuse_albedo, info.diffuse_albedo_format),
        ),
        specular_albedo: TextureViewRef::from_texture(
            &specular_albedo,
            check_view_use_format(&specular_albedo, info.specular_albedo_format),
        ),
        normal: TextureViewRef::from_texture(
            &normal,
            check_view_use_format(&normal, info.normal_format),
        ),
        roughness: TextureViewRef::from_texture(
            &roughness,
            check_view_use_format(&roughness, info.roughness_format),
        ),
        specular_hit_distance: view_for_format(
            specular_hit_distance.as_ref(),
            info.specular_hit_distance_format,
        ),
        denoise_strength_mask: view_for_format(
            denoise_strength_mask.as_ref(),
            info.denoise_strength_mask_format,
        ),
        transparency_overlay: view_for_format(
            transparency_overlay.as_ref(),
            info.transparency_overlay_format,
        ),
        exposure: exposure_tex
            .as_ref()
            .and_then(|t| TextureViewRef::from_texture(t, t.full_view())),
        reactive: view_for_format(mask.as_ref(), info.reactive_mask_format),
        depth_scratch,
        motion_scratch,
        output_scratch: output_scratch.clone(),
        diffuse_albedo_scratch,
        specular_albedo_scratch,
        normal_scratch,
        roughness_scratch,
        specular_hit_distance_scratch,
        denoise_strength_mask_scratch,
        transparency_overlay_scratch,
        reactive_scratch,
        props,
    };
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
    // `TemporalDenoisedUpscaleEncoderData`.
    unsafe { ptr::write(ptr, init) };
    alloc.encoder_current = ptr.cast::<EncoderData>();
    if let Some(ref scratch) = output_scratch {
        alloc.writes(Arc::as_ptr(scratch).cast::<c_void>());
    } else {
        // SAFETY: `d.output` is a valid non-null `Resource` pointer.
        let dst_key = unsafe { copy::resource_key(d.output) };
        alloc.writes(dst_key);
    }
    alloc.invalidate_current_pass();

    if let Some(scratch) = output_scratch {
        // SAFETY: `d.output` is a valid non-null `Resource` pointer.
        let dst_key = unsafe { copy::resource_key(d.output) };
        copy::pre_blit(alloc, dst_key, false);
        let bytes = format::texel_size(output.info().pixel_format);
        let src_fmt = as_format_desc(info.output_format, bytes);
        let dst_fmt = as_format_desc(output.info().pixel_format, bytes);
        copy::copy_reinterpret(
            alloc,
            &copy::CopyReinterpretSide {
                texture: &scratch,
                format: &src_fmt,
                level: 0,
                slice: 0,
                origin: WMTOrigin { x: 0, y: 0, z: 0 },
            },
            WMTSize {
                width: u64::from(sd.output_width),
                height: u64::from(sd.output_height),
                depth: 1,
            },
            &copy::CopyReinterpretSide {
                texture: &output,
                format: &dst_fmt,
                level: 0,
                slice: 0,
                origin: WMTOrigin { x: 0, y: 0, z: 0 },
            },
        );
    }

    S_OK
}

/// `IMTLD3D12CommandListExt1::SetFrameGenUI`.
///
/// # Safety
///
/// `this` is the `IMTLD3D12CommandListExt1` companion pointer at `List::EXT_OFFSET`; `desc` is a
/// caller-supplied pointer.
pub unsafe fn set_frame_gen_ui(this: *mut c_void, desc: *const D3D12FrameGenUiDesc) -> Hresult {
    crate::scope!("list.SetFrameGenUI");
    if this.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` points to `ext_vtbl` inside `List` at `List::EXT_OFFSET`.
    let base_ptr = unsafe { this.byte_sub(List::EXT_OFFSET).cast::<List>() };
    // SAFETY: `base_ptr` is the enclosing `List` allocation.
    let list = unsafe { &mut *base_ptr };
    // SAFETY: `desc` was validated non-null above.
    let d = unsafe { desc.read_unaligned() };
    let ui = FrameGenUi {
        ui_composited: d.ui_composited != 0,
        content_width: d.content_width,
        content_height: d.content_height,
        depth_offset: d.depth_offset,
        motion_offset: d.motion_offset,
        output_offset: d.output_offset,
        has_matrices: d.has_matrices != 0,
        world_to_view_matrix: d.world_to_view_matrix,
        view_to_clip_matrix: d.view_to_clip_matrix,
    };
    if d.ui_texture.is_null() {
        framegen::record_frame_gen_ui(None, &ui);
        return S_OK;
    }
    // SAFETY: `d.ui_texture` is a non-null caller-supplied `ID3D12Resource` pointer.
    let res_obj = unsafe { &*d.ui_texture.cast::<Resource>() };
    let Some(tex) = res_obj.texture.as_ref() else {
        return E_INVALIDARG;
    };
    let fmt = tex.info().pixel_format;
    if tex.info().r#type() != WMT_TEXTURE_TYPE2D || fmt == WMT_PIXEL_FORMAT_INVALID {
        return E_INVALIDARG;
    }
    let Some(view_ref) = TextureViewRef::from_texture(tex, tex.full_view()) else {
        return E_INVALIDARG;
    };
    // SAFETY: `view_ref.texture()` is a live `MTLTexture` handle kept alive by `view_ref`.
    let Some(obj) = (unsafe { Obj::retain(view_ref.texture()) }) else {
        return E_INVALIDARG;
    };
    if !list.allocator.is_null() {
        // SAFETY: `list.allocator` was checked non-null.
        let alloc = unsafe { &mut *list.allocator };
        alloc.keep_texture(Arc::clone(tex));
    }
    framegen::record_frame_gen_ui(Some((&obj, fmt)), &ui);
    S_OK
}

#[cfg(test)]
mod tests;
