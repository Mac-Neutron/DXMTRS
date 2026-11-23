//! `StretchRect`'s sampled copy on the core (DXMTRS C4c-4c): mtld3d's blit quad, a pass of its own.
//!
//! One render pass on the destination's view samples one subresource of the source through the
//! internal command library's `mtld3d_blit_vs` and `mtld3d_blit_ps` functions: the copy that
//! scales, converts, decodes packed YUV or writes a multisampled target. The functions are loaded
//! once per device, at its first such copy, and its pipelines are kept by the destination's
//! stored format and sample count. The pass is recorded here whole: the pass, the pipeline, the
//! viewport, the scissor, the rasterizer state and the draw repeat what `draw.rs` and `open_pass`
//! encode for a draw, kept apart so the draw path is left as it is.

use std::{ffi::CStr, sync::Arc};

use dxmt_core::{
    deptrack::Access,
    device::Device,
    encoder::{Encoder, NO_NEXT, direct, stage},
    library::{render_pipeline, render_pipeline_info},
    resource::TextureViewDescriptor,
    sampler::Sampler,
};
use mtld3d_core::{
    sampler_state::{description_from_snapshot, key_from_snapshot, snapshot_from_state},
    stretch_rect::{BLIT_PS, BLIT_VS, BlitDecode},
};
use mtld3d_types::{
    D3DSAMP_ADDRESSU, D3DSAMP_ADDRESSV, D3DSAMP_ADDRESSW, D3DSAMP_MAGFILTER, D3DSAMP_MINFILTER,
    D3DSAMP_MIPFILTER, D3DTADDRESS_CLAMP, D3DTEXF_LINEAR, D3DTEXF_POINT, SAMPLER_STATE_COUNT,
};
use winemetal_sys::{
    MTLLibrary_newFunction, Obj,
    wire::{
        MemPtr, WMT_CULL_MODE_NONE, WMT_DEPTH_CLIP_MODE_CLIP, WMT_LOAD_ACTION_LOAD,
        WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
        WMT_RENDER_COMMAND_SET_VIEWPORTS, WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D,
        WMT_TRIANGLE_FILL_MODE_FILL, WMT_WINDING_CLOCKWISE, WMTScissorRect, WMTViewport,
        WmtcmdRenderDraw, WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate,
        WmtcmdRenderSetscissorrects, WmtcmdRenderSetviewports, original_format,
    },
};

use super::{CoreDevice, LOG_TARGET, TargetRef, query, surface::inside, target::Shape};

/// One sampled copy: a rect of a source's subresource onto a rect of a target.
pub struct StretchQuad {
    /// The source: any stored colour level (a target or not), or the back buffer.
    pub src: TargetRef,
    /// `[x, y, width, height]` in the texels of the source's level.
    pub src_rect: [u32; 4],
    /// The `WMTPixelFormat` the source is sampled through, swizzle bits included.
    ///
    /// Its stored format, its sRGB twin or a swizzle of either: mtld3d samples a level of a
    /// texture that is no target through its swizzled view, every other source through its
    /// stored format.
    pub src_view: u32,
    /// The destination: a level or face of a render-target texture, or the back buffer.
    ///
    /// A standalone target is a render-target texture's one level; any of them may be
    /// multisampled.
    pub dst: TargetRef,
    /// `[x, y, width, height]` in the texels of the destination's level.
    pub dst_rect: [u32; 4],
    /// Bilinear sampling (`D3DTEXF_LINEAR`); nearest otherwise.
    pub linear: bool,
    /// The decode the shader applies: [`BlitDecode::uniform`]'s number.
    pub decode: f32,
}

impl CoreDevice {
    /// Samples a rect of the source onto a rect of the target: `StretchRect`'s quad (C4c-4c).
    ///
    /// One render pass of its own on the destination level's view in its stored format, never its
    /// sRGB twin: the copy is verbatim, as mtld3d's. The pass loads and stores the destination, its
    /// size the level's extent, so a Clear of the destination recorded before folds into it. Its
    /// viewport and scissor are `quad.dst_rect`, its rasterizer state fills and culls nothing, and
    /// one triangle samples `quad.src_rect` through a one-level, one-slice 2D view of the source's
    /// subresource in `quad.src_view`, its read tracked, with a clamped sampler, bilinear for
    /// `quad.linear` and point otherwise. A back buffer source is the one Present shows: a
    /// multisampled one is first resolved into the presented texture as Present resolves it. The
    /// open pass ends first, no pass is open after, and the destination counts as written through
    /// its stored format.
    ///
    /// False, and nothing recorded, when the source does not resolve or is a multisampled texture,
    /// the destination is no target, a rect is empty or leaves its level, both ends are one
    /// texture (a pass does not sample what it attaches), `quad.decode` is a planar one (the
    /// core's planar texture holds its luma alone), `quad.src_view`'s base format is neither the
    /// source's stored format nor its twin or differs from the stored format on a texture made
    /// without views in another format, or Metal makes no function, pipeline or sampler for it.
    /// Functions are loaded only once nothing else refuses: a refused call loads nothing.
    #[must_use]
    pub fn stretch(&mut self, quad: &StretchQuad) -> bool {
        let (Some((from, shape, resolve_first)), Some(to)) =
            (self.copy_source(quad.src), self.resolve(quad.dst))
        else {
            return false;
        };
        let into = to.shape;
        let [x, y, width, height] = quad.src_rect;
        let [dst_x, dst_y, dst_width, dst_height] = quad.dst_rect;
        let base = original_format(quad.src_view);
        let stored_or_twin =
            base == shape.pixel_format || (shape.srgb_format != 0 && base == shape.srgb_format);
        let viewable = match quad.src {
            TargetRef::BackBuffer => true,
            TargetRef::Texture { id, .. } => self.view_twins(id),
            TargetRef::Null => false,
        };
        let planar = [BlitDecode::Yv12.uniform(), BlitDecode::Nv12.uniform()]
            .iter()
            .any(|decode| decode.to_bits() == quad.decode.to_bits());
        if shape.samples > 1
            || width == 0
            || height == 0
            || !inside(x, width, shape.width)
            || !inside(y, height, shape.height)
            || dst_width == 0
            || dst_height == 0
            || !inside(dst_x, dst_width, into.width)
            || !inside(dst_y, dst_height, into.height)
            || Arc::ptr_eq(&from, &to.texture)
            || planar
            || !stored_or_twin
            || (quad.src_view != shape.pixel_format && !viewable)
        {
            return false;
        }
        let Some(pso) = self.quad_pipeline(into.pixel_format, into.samples) else {
            return false;
        };
        let Some(sampler) = self.quad_sampler(quad.linear) else {
            return false;
        };
        if resolve_first {
            self.resolve_presented();
        }
        self.close_pass();
        let view = TextureViewDescriptor {
            format: quad.src_view,
            texture_type: WMT_TEXTURE_TYPE2D,
            first_mip: shape.level,
            mip_count: 1,
            first_slice: shape.slice,
            array_size: 1,
        }
        .key();
        let (xform, src_level) = quad_uniforms(&shape, quad.src_rect, quad.decode);
        // Each `float4` as the 16 little-endian bytes the shader reads.
        let (xform, src_level) = (xform.map(f32::to_le_bytes), src_level.map(f32::to_le_bytes));
        let dst_rect = quad.dst_rect;
        let (target, key, extent) = (to.texture, to.key, (into.width, into.height));
        let counted = self.open_brackets.is_some();
        self.emit(move |enc| {
            enc.start_render_pass(0, 0, 1, None);
            let attachment =
                enc.attachment::<{ stage::PIXEL }>(&target, key, Access::READ | Access::WRITE);
            if let Some(targets) = enc.render_targets() {
                let color = &mut targets.colors[0];
                color.target.attachment = attachment;
                color.target.load_action = WMT_LOAD_ACTION_LOAD;
                color.target.store_action = WMT_STORE_ACTION_STORE;
                (targets.render_target_width, targets.render_target_height) = extent;
            }
            encode_triangle_state(enc, pso, dst_rect, counted);
            // Metal's slot 0 of each stage, as the shader declares them: a D3D9 pass binds no
            // argument buffer that these would have to leave alone.
            direct::bind_bytes::<{ stage::VERTEX }>(enc, 0, xform.as_flattened());
            direct::bind_bytes::<{ stage::PIXEL }>(enc, 0, src_level.as_flattened());
            direct::bind_texture::<{ stage::PIXEL }>(enc, 0, Some((&*from, view)));
            direct::bind_sampler::<{ stage::PIXEL }>(enc, 0, Some(&sampler), false);
            enc.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            });
            enc.end_pass();
        });
        self.note_written(quad.dst, into.pixel_format);
        true
    }

    /// The quad's pipeline into `format` at `samples`, made at its first request.
    ///
    /// Through dxmt-core's `render_pipeline`, as its own clears make theirs: the quad's two
    /// functions, colour 0 in `format`, no blending, no depth. One Metal does not make is logged
    /// and kept as none, never made again; none when the command library or a function is missing.
    pub(super) fn quad_pipeline(&mut self, format: u32, samples: u8) -> Option<u64> {
        if let Some(pso) = self.quad_pipelines.get(&(format, samples)) {
            return pso.as_ref().map(Obj::handle);
        }
        let [vs, ps] = self
            .quad_functions
            .get_or_init(|| {
                load_internal_functions(
                    &self.device,
                    [BLIT_VS, BLIT_PS],
                    "the StretchRect quad",
                    "sampled copies",
                )
            })
            .as_ref()?
            .each_ref()
            .map(Obj::handle);
        let mut info = render_pipeline_info();
        info.vertex_function = vs;
        info.fragment_function = ps;
        info.colors[0].pixel_format = format;
        info.raster_sample_count = samples;
        let pso = render_pipeline(self.device.device(), &info);
        if pso.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: the StretchRect quad's pipeline into format {format} at {samples} \
                 samples does not build; those sampled copies are refused"
            );
        }
        let handle = pso.as_ref().map(Obj::handle);
        self.quad_pipelines.insert((format, samples), pso);
        handle
    }

    /// The quad's sampler, the cache's: min and mag bilinear for `linear` or point, mip point.
    ///
    /// Every address clamped to the edge, so a rect at its texture's edge reads nothing from the
    /// other side: mtld3d's blit sampler, a D3D9 sampler state of those values keyed and
    /// described as a draw's.
    pub(super) fn quad_sampler(&mut self, linear: bool) -> Option<Arc<Sampler>> {
        let filter = if linear {
            D3DTEXF_LINEAR
        } else {
            D3DTEXF_POINT
        };
        let mut states = [0; SAMPLER_STATE_COUNT];
        states[D3DSAMP_MINFILTER as usize] = filter;
        states[D3DSAMP_MAGFILTER as usize] = filter;
        // The shader samples level 0 of a one-level view, which the point mip filter keeps exact.
        states[D3DSAMP_MIPFILTER as usize] = D3DTEXF_POINT;
        for address in [D3DSAMP_ADDRESSU, D3DSAMP_ADDRESSV, D3DSAMP_ADDRESSW] {
            states[address as usize] = D3DTADDRESS_CLAMP;
        }
        let snapshot = snapshot_from_state(&states, false);
        let key = key_from_snapshot(&snapshot);
        self.sampler(key, &description_from_snapshot(&snapshot, key))
    }
}

/// Records a pass's pipeline `pso`, viewport and scissor on `rect`, and its rasterizer state.
///
/// What a pass of the device's own sets before its one triangle (the `StretchRect` quad, the
/// depth-stencil resolve): `rect` is `x, y, width, height`, the viewport's depth runs from 0 to
/// 1, and the rasterizer state fills, culls nothing, clips depth and has no bias. The state is
/// the pass's own: the optimizer may merge such a pass with a draw's on the same view, and
/// Metal's encoder would keep that draw's cull and fill modes. For the same reason, once the
/// device began an occlusion query (`counted`), counting is turned off first (DXMTRS C4c-5).
pub(super) fn encode_triangle_state(enc: &mut Encoder, pso: u64, rect: [u32; 4], counted: bool) {
    if counted {
        query::visibility_off(enc);
    }
    let [x, y, width, height] = rect;
    enc.encode_render_command(WmtcmdRenderSetpso {
        r#type: WMT_RENDER_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: NO_NEXT,
        pso,
    });
    let slot = enc
        .allocate_cpu(size_of::<WMTViewport>(), align_of::<WMTViewport>())
        .cast::<WMTViewport>();
    let viewport = WMTViewport {
        origin_x: f64::from(x),
        origin_y: f64::from(y),
        width: f64::from(width),
        height: f64::from(height),
        znear: 0.0,
        zfar: 1.0,
    };
    // SAFETY: the encoder's memory, sized and aligned for one viewport, nobody else's.
    unsafe { slot.write(viewport) };
    enc.encode_render_command(WmtcmdRenderSetviewports {
        r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
        reserved: [0; 3],
        next: NO_NEXT,
        viewports: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
        viewport_count: 1,
    });
    let slot = enc
        .allocate_cpu(size_of::<WMTScissorRect>(), align_of::<WMTScissorRect>())
        .cast::<WMTScissorRect>();
    let scissor = WMTScissorRect {
        x: u64::from(x),
        y: u64::from(y),
        width: u64::from(width),
        height: u64::from(height),
    };
    // SAFETY: the encoder's memory, sized and aligned for one rect, nobody else's.
    unsafe { slot.write(scissor) };
    enc.encode_render_command(WmtcmdRenderSetscissorrects {
        r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
        reserved: [0; 3],
        next: NO_NEXT,
        scissor_rects: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
        rect_count: 1,
    });
    enc.encode_render_command(WmtcmdRenderSetrasterizerstate {
        r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
        reserved: [0; 3],
        next: NO_NEXT,
        fill_mode: WMT_TRIANGLE_FILL_MODE_FILL,
        cull_mode: WMT_CULL_MODE_NONE,
        depth_clip_mode: WMT_DEPTH_CLIP_MODE_CLIP,
        winding: WMT_WINDING_CLOCKWISE,
        depth_bias: 0.0,
        scole_scale: 0.0,
        depth_bias_clamp: 0.0,
    });
}

/// Loads the functions `names` from `device`'s internal command library, in that order.
///
/// Logs and returns `None` when a live device has no command library or a function is missing;
/// callers latch the `None` and do not retry. On `dxmt-fakes` unit-test devices (`<= 0x1000`),
/// `cmd_library_handle()` is `0` and `MTLLibrary_newFunction` returns fake handles.
pub(super) fn load_internal_functions<const N: usize>(
    device: &Device,
    names: [&CStr; N],
    what: &str,
    refused: &str,
) -> Option<[Obj; N]> {
    let lib = device.cmd_library_handle();
    if lib == 0 && device.device().handle() > 0x1000 {
        log::error!(
            target: LOG_TARGET,
            "d3d9 core: no command library for {what}; {refused} are refused"
        );
        return None;
    }
    let function = |name: &CStr| {
        // SAFETY: `lib` is the device's live command library (or `0` on `dxmt-fakes`), and `name`
        // is NUL-terminated for the call.
        let function = unsafe { MTLLibrary_newFunction(lib, name.as_ptr()) };
        // SAFETY: a new function's handle carries its reference.
        let function = unsafe { Obj::adopt(function) };
        if function.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: missing {name:?} in command library for {what}; {refused} are refused"
            );
        }
        function
    };
    // None from the first function Metal does not make: the ones after it are not asked for.
    names
        .into_iter()
        .map(function)
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}

/// The quad's two uniforms: the vertex stage's `xform` and the fragment stage's `src_level`.
///
/// `xform` maps the quad across `rect` (`x, y, width, height`) of the source level `src`,
/// divided by that level's own extent, as mtld3d divides by the extent of the subresource it
/// addresses. `src_level` is level 0 (the source is a one-level view of its level), `decode`,
/// and the level's extent, which only the planar decodes read. Exact for every extent D3D9 has
/// (at most 16384); a larger one reads 65535, as mtld3d's does.
fn quad_uniforms(
    src: &Shape,
    [x, y, width, height]: [u32; 4],
    decode: f32,
) -> ([f32; 4], [f32; 4]) {
    let to_f = |value: u32| f32::from(u16::try_from(value).unwrap_or(u16::MAX));
    let (level_width, level_height) = (to_f(src.width), to_f(src.height));
    let (across, down) = (level_width.max(1.0), level_height.max(1.0));
    (
        [
            to_f(width) / across,
            to_f(height) / down,
            to_f(x) / across,
            to_f(y) / down,
        ],
        [0.0, decode, level_width, level_height],
    )
}

#[cfg(test)]
mod tests;
