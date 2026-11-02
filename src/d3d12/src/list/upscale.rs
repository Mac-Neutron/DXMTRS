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
    resource::{Texture, TextureAllocationFlags, TextureViewKey},
};
use winemetal_wire::{
    WMT_PIXEL_FORMAT_INVALID, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_RENDER_TARGET,
    WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTFXTemporalScalerProps,
    WMTOrigin, WMTSize,
};

use crate::{
    abi::ext::D3D12UpscaleDesc,
    com::{E_INVALIDARG, Hresult, S_OK},
    device::temporal::{Scratch, TemporalScalerObject},
    encoder::{EncoderData, EncoderType, TemporalUpscaleEncoderData, TextureViewRef},
    list::{List, copy},
    resource::Resource,
};

const WRITABLE_USAGE: u32 = WMT_TEXTURE_USAGE_SHADER_READ
    | WMT_TEXTURE_USAGE_SHADER_WRITE
    | WMT_TEXTURE_USAGE_RENDER_TARGET;

const fn as_format_desc(pixel_format: u32, bytes_per_texel: u32) -> MtlDxgiFormatDesc {
    MtlDxgiFormatDesc {
        pixel_format,
        attribute_format: 0,
        bytes_per_texel,
        planar_count: 0,
        flag: 0,
    }
}

fn take_texture(
    resource: *mut c_void,
    format: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<Arc<Texture>> {
    if resource.is_null() {
        return None;
    }
    // SAFETY: `resource` is a caller-supplied `ID3D12Resource` pointer.
    let res_obj = unsafe { &*resource.cast::<Resource>() };
    let tex = res_obj.texture.as_ref()?;
    if tex.info().r#type() != WMT_TEXTURE_TYPE2D
        || res_obj.desc_scoped().format != format
        || u64::from(x) + u64::from(w) > u64::from(tex.info().width)
        || u64::from(y) + u64::from(h) > u64::from(tex.info().height)
    {
        return None;
    }
    Some(Arc::clone(tex))
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
    // SAFETY: `scaler` was checked non-null above.
    let scaler = unsafe { &*scaler.cast::<TemporalScalerObject>() };
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
    let mask = if masked {
        take_texture(
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
        || mask
            .as_ref()
            .is_some_and(|m| !readable(m, info.reactive_mask_format))
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

    let mut depth_scratch: Option<Arc<Texture>> = None;
    let mut motion_scratch: Option<Arc<Texture>> = None;
    let mut output_scratch: Option<Arc<Texture>> = None;

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !readable(&depth, info.depth_format) || !readable(&motion, info.motion_format) {
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
        reactive: mask
            .as_ref()
            .and_then(|t| TextureViewRef::from_texture(t, t.full_view())),
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

#[cfg(test)]
mod tests;
