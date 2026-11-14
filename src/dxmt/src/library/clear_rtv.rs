//! Clearing a render target, or a depth-stencil one, with a draw.

use std::{cmp::max, ffi::CStr, sync::Arc};

use rustc_hash::FxHashMap;
use winemetal_sys::{
    Obj,
    wire::{
        WMT_COMPARE_FUNCTION_ALWAYS, WMT_PIXEL_FORMAT_INVALID,
        WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE, WMT_PRIMITIVE_TYPE_TRIANGLE,
        WMT_STENCIL_OPERATION_REPLACE, WMTDepthStencilInfo, WMTRenderPipelineInfo, WMTScissorRect,
        WMTStencilInfo, WMTViewport,
    },
};

use super::{
    CUSTOM_BUFFER_ARGUMENT_INDEX_0, SimpleCommandContext, depth_stencil_state, render_pipeline,
    render_pipeline_info,
};
use crate::{
    format,
    resource::{Texture, TextureViewKey},
};

/// A clear of one view of one target, one `begin` to its `end`.
///
/// `begin` starts the render pass and makes (once per format, sample count and depth-stencil
/// planes) the pipeline that draws the colour; each clear draws it over one rectangle.
pub struct ClearRtv {
    device: Obj,
    vertex: Option<Obj>,
    fragment_depth: Option<Obj>,
    fragment_float: Option<Obj>,
    fragment_sint: Option<Obj>,
    fragment_uint: Option<Obj>,
    /// Depth-stencil states, in the order of [`depth_stencil_infos`].
    states: [Option<Obj>; 4],
    /// A pipeline per [`pso_key`]; `None` where Metal made none, which is not tried again.
    pipelines: FxHashMap<u64, Option<Obj>>,
    clearing: Option<(Arc<Texture>, TextureViewKey)>,
}

impl ClearRtv {
    /// A clear on `device`, whose shaders are `ctx`'s default library.
    #[must_use]
    pub fn new<C: SimpleCommandContext>(device: &Obj, ctx: &C) -> Self {
        let library = ctx.default_library();
        let function = |name: &CStr| library.function(name);
        // SAFETY: `device` holds a reference, so its handle is a live object's.
        let device_ref = unsafe { Obj::retain(device.handle()) };
        Self {
            device: device_ref.unwrap_or_else(|| unreachable!("an Obj's handle is never null")),
            vertex: function(c"vs_clear_rt"),
            fragment_depth: function(c"fs_clear_rt_depth"),
            fragment_float: function(c"fs_clear_rt_float"),
            fragment_sint: function(c"fs_clear_rt_sint"),
            fragment_uint: function(c"fs_clear_rt_uint"),
            states: depth_stencil_infos().map(|info| depth_stencil_state(device, &info)),
            pipelines: FxHashMap::default(),
            clearing: None,
        }
    }

    /// Begins clearing `view` of `texture`.
    ///
    /// It clears the depth-stencil planes `dsv_flag` names (bit 0 depth, bit 1 stencil), or else
    /// the colour at `depth_plane`.
    ///
    /// Nothing begins, and the clears and `end` do nothing, if Metal cannot make the pipeline.
    pub fn begin<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        texture: Arc<Texture>,
        view: TextureViewKey,
        depth_plane: u32,
        dsv_flag: u32,
    ) {
        debug_assert!(self.clearing.is_none(), "a clear is already begun");
        let format = view.format();
        let sample_count = texture.info().sample_count();
        let key = pso_key(dsv_flag, sample_count, format);
        if !self.pipelines.contains_key(&key) {
            let fragment = match kind(format, dsv_flag) {
                Kind::Depth => &self.fragment_depth,
                Kind::Float => &self.fragment_float,
                Kind::Sint => &self.fragment_sint,
                Kind::Uint => &self.fragment_uint,
            };
            let info = pipeline_info(
                self.vertex.as_ref().map_or(0, Obj::handle),
                fragment.as_ref().map_or(0, Obj::handle),
                format,
                sample_count,
                dsv_flag,
            );
            self.pipelines
                .insert(key, render_pipeline(&self.device, &info));
        }
        let Some(Some(pso)) = self.pipelines.get(&key) else {
            return;
        };
        let [width, height, _] = texture.view_size(view);
        ctx.start_render_pass();
        if dsv_flag == 0 {
            ctx.set_color_attachment(0, &texture, view, depth_plane);
        } else {
            ctx.set_depth_stencil_attachment(&texture, view, dsv_flag);
        }
        ctx.set_render_pso(pso);
        ctx.set_viewport(WMTViewport {
            origin_x: 0.0,
            origin_y: 0.0,
            width: f64::from(width),
            height: f64::from(height),
            znear: 0.0,
            zfar: 1.0,
        });
        let state = match dsv_flag {
            3 => &self.states[2],
            2 => &self.states[3],
            1 => &self.states[1],
            _ => &self.states[0],
        };
        if let Some(state) = state {
            ctx.set_depth_stencil_state(state);
        }
        self.clearing = Some((texture, view));
    }

    /// Clears `rect` of every one of `array_length` slices (one if 0) to `color`.
    pub fn clear_color<C: SimpleCommandContext>(
        &self,
        ctx: &mut C,
        rect: WMTScissorRect,
        array_length: u32,
        color: [f32; 4],
    ) {
        if self.clearing.is_none() {
            return;
        }
        ctx.set_scissor_rect(rect);
        ctx.set_fragment_bytes(
            CUSTOM_BUFFER_ARGUMENT_INDEX_0,
            color.map(f32::to_le_bytes).as_flattened(),
        );
        ctx.draw(WMT_PRIMITIVE_TYPE_TRIANGLE, 0, 3, 0, max(array_length, 1));
    }

    /// Clears `rect` of every slice of the view to `depth` and `stencil`.
    pub fn clear_depth_stencil<C: SimpleCommandContext>(
        &self,
        ctx: &mut C,
        rect: WMTScissorRect,
        depth: f32,
        stencil: u8,
    ) {
        let Some((_, view)) = &self.clearing else {
            return;
        };
        ctx.set_scissor_rect(rect);
        ctx.set_fragment_bytes(
            CUSTOM_BUFFER_ARGUMENT_INDEX_0,
            [depth; 4].map(f32::to_le_bytes).as_flattened(),
        );
        ctx.set_stencil_reference(stencil);
        ctx.draw(
            WMT_PRIMITIVE_TYPE_TRIANGLE,
            0,
            3,
            0,
            max(view.array_size(), 1),
        );
    }

    /// Ends the pass `begin` started; nothing if none was begun.
    pub fn end<C: SimpleCommandContext>(&mut self, ctx: &mut C) {
        if self.clearing.take().is_some() {
            ctx.end_pass();
        }
    }
}

/// Which fragment shader draws the clear value.
#[derive(PartialEq, Eq, Debug)]
enum Kind {
    Depth,
    Float,
    Sint,
    Uint,
}

/// The shader for a target of `format`.
///
/// The depth one if `dsv_flag` names the depth plane, else by the format's numeric kind. A
/// stencil-only clear takes the float one: its pipeline has no depth format, which a fragment
/// that writes depth needs, and the colour it writes has no attachment (the stencil is the
/// reference value's).
const fn kind(format: u32, dsv_flag: u32) -> Kind {
    if dsv_flag & 1 != 0 {
        Kind::Depth
    } else if dsv_flag != 0 || !format::is_integer(format) {
        Kind::Float
    } else if format::unsigned_integer(format) == format {
        Kind::Uint
    } else {
        Kind::Sint
    }
}

/// What tells two clear pipelines apart: the planes, the sample count and the format.
fn pso_key(dsv_flag: u32, sample_count: u32, format: u32) -> u64 {
    u64::from(dsv_flag & 0x3)
        | (u64::from(sample_count & 0x3fff_ffff) << 2)
        | (u64::from(format) << 32)
}

/// The pipeline that draws `fragment` over a target of `format` and `sample_count` samples.
fn pipeline_info(
    vertex: u64,
    fragment: u64,
    format: u32,
    sample_count: u32,
    dsv_flag: u32,
) -> WMTRenderPipelineInfo {
    let mut info = render_pipeline_info();
    info.raster_sample_count = u8::try_from(sample_count).unwrap_or(1);
    info.vertex_function = vertex;
    info.fragment_function = fragment;
    if dsv_flag == 0 {
        info.colors[0].pixel_format = format;
    } else {
        let plane = |bit| {
            if dsv_flag & bit != 0 {
                format
            } else {
                WMT_PIXEL_FORMAT_INVALID
            }
        };
        info.depth_pixel_format = plane(1);
        info.stencil_pixel_format = plane(2);
    }
    info.rasterization_enabled = 1;
    info.input_primitive_topology = WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE;
    info
}

/// The four depth-stencil states [`ClearRtv`] keeps.
///
/// In order: depth tested and not written, depth written, depth and stencil written, stencil
/// written. The stencil replaces on every outcome.
const fn depth_stencil_infos() -> [WMTDepthStencilInfo; 4] {
    const fn state(depth_write_enabled: u8, front_stencil: WMTStencilInfo) -> WMTDepthStencilInfo {
        WMTDepthStencilInfo {
            depth_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
            depth_write_enabled,
            front_stencil,
            back_stencil: OFF,
        }
    }
    const OFF: WMTStencilInfo = WMTStencilInfo {
        enabled: 0,
        depth_stencil_pass_op: 0,
        stencil_fail_op: 0,
        depth_fail_op: 0,
        stencil_compare_function: 0,
        write_mask: 0,
        read_mask: 0,
    };
    const REPLACE: WMTStencilInfo = WMTStencilInfo {
        enabled: 1,
        depth_stencil_pass_op: WMT_STENCIL_OPERATION_REPLACE,
        stencil_fail_op: WMT_STENCIL_OPERATION_REPLACE,
        depth_fail_op: WMT_STENCIL_OPERATION_REPLACE,
        stencil_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
        write_mask: 0xff,
        read_mask: 0xff,
    };
    [
        state(0, OFF),
        state(1, OFF),
        state(1, REPLACE),
        state(0, REPLACE),
    ]
}

#[cfg(test)]
mod tests;
