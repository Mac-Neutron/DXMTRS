//! The presenter: the swapchain's layer, its colour space and the pass that draws a frame to it.
//!
//! A [`Presenter`] holds the 1024×1 gamma lookup texture and the two present pipelines (the
//! backbuffer at the drawable's size, or scaled to it), which it rebuilds when the layer's format,
//! colour space, sample count, HDR metadata, gamma ramp or the display's setting changed, after
//! every frame it handed out is done. The C++ `Presenter` owns one through the
//! `dxmt_presenter_*` functions its header `dxmt_presenter.hpp` declares.
//!
//! What the encoding thread reads sits behind one lock, held for a snapshot and released before
//! any Metal call or callback; the frames handed out are counted behind another, which a rebuild
//! waits on without holding the first.

mod abi;

use core::{
    ffi::c_void,
    mem::ManuallyDrop,
    ptr::{from_ref, null},
    sync::atomic::{
        AtomicU32,
        Ordering::{Acquire, Relaxed},
    },
};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use dxmt_util::{
    GAMMA_CP_COUNT, GammaRamp as DXMTGammaRamp,
    float_cast::{f32_to_u32, f64_to_f32, u32_to_f32, u64_to_f64},
};
use winemetal_sys::{
    MTLCommandBuffer_renderCommandEncoder, MTLCommandEncoder_endEncoding,
    MTLDevice_newRenderPipelineState, MTLDevice_newTexture, MTLLibrary_newFunction,
    MTLLibrary_newFunctionWithConstants, MTLRenderCommandEncoder_encodeCommands, MTLTexture_height,
    MTLTexture_replaceRegion, MTLTexture_width, MetalDrawable_texture, MetalLayer_getProps,
    MetalLayer_nextDrawable, MetalLayer_setColorSpace, MetalLayer_setProps, Obj, Pool,
    WMTQueryDisplaySettingForLayer,
    wire::{
        MemPtr, WMT_BLEND_FACTOR_ONE, WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD,
        WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB, WMT_COLOR_SPACE_SRGB,
        WMT_COLOR_WRITE_MASK_ALL, WMT_DATA_TYPE_BOOL, WMT_LOAD_ACTION_CLEAR,
        WMT_LOAD_ACTION_DONT_CARE, WMT_LOGIC_OPERATION_CLEAR, WMT_PIXEL_FORMAT_INVALID,
        WMT_PIXEL_FORMAT_RGBA32_FLOAT, WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED,
        WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VIEWPORTS,
        WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_STORE_ACTION_STORE,
        WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT, WMT_TESSELLATION_PARTITION_MODE_POW2,
        WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_SHADER_READ, WMT_WINDING_CLOCKWISE,
        WMTColorAttachmentBlendInfo, WMTEDRValue, WMTFunctionConstant, WMTHDRMetadata,
        WMTLayerProps, WMTOrigin, WMTRenderPassInfo, WMTRenderPipelineInfo, WMTSize,
        WMTTextureInfo, WMTViewport, WmtcmdBase, WmtcmdRenderDraw, WmtcmdRenderSetbytes,
        WmtcmdRenderSetpso, WmtcmdRenderSettexture, WmtcmdRenderSetviewports,
    },
};

use crate::format::forget_srgb;

/// `DXMTPresentMetadata`: what the present pass's fragment function reads as its bytes.
#[repr(C)]
pub struct DXMTPresentMetadata {
    pub edr_scale: f32,
    pub max_content_luminance: f32,
    pub max_display_luminance: f32,
}

/// `dxmt_presenter_callbacks`: the two callbacks `dxmt_presenter_encode` runs on the encoder.
///
/// Each is called once, during that call, with `ctx` and the present pass's render encoder:
/// `wait` before the pass's commands, `update` after them.
#[repr(C)]
pub struct DxmtPresenterCallbacks {
    pub wait: unsafe extern "C" fn(ctx: *mut c_void, encoder: u64),
    pub update: unsafe extern "C" fn(ctx: *mut c_void, encoder: u64),
    pub ctx: *mut c_void,
}

/// What a presenter is made for: borrowed objects that outlive it, and two numbers.
pub struct PresenterDesc {
    /// The `MTLDevice`.
    pub device: u64,
    /// The swapchain's `CAMetalLayer`.
    pub layer: u64,
    /// DXMT's internal command library, which holds the present functions.
    pub library: u64,
    /// The layer's contents scale is multiplied by it.
    pub scale_factor: f32,
    /// The backbuffer's sample count.
    pub sample_count: u8,
}

/// A swapchain's presenter (`dxmt_presenter.cpp`).
pub struct Presenter {
    device: u64,
    layer: u64,
    library: u64,
    // Fields drop in declaration order: the C++ released the pipelines, then the lookup texture.
    state: Mutex<State>,
    gamma_lut_texture: Option<Obj>,
    /// The newest frame done: frames end in order of their ids, as `CpuFence` counts them.
    presented: Mutex<u64>,
    frame_done: Condvar,
}

impl Presenter {
    /// The presenter of `desc.layer`.
    ///
    /// It takes the layer's properties and makes its lookup texture.
    ///
    /// # Safety
    ///
    /// `desc`'s device, layer and library are live and outlive the presenter.
    #[must_use]
    pub unsafe fn new(desc: &PresenterDesc) -> Self {
        let mut layer_props = WMTLayerProps {
            device: 0,
            contents_scale: 0.0,
            drawable_width: 0.0,
            drawable_height: 0.0,
            opaque: 0,
            display_sync_enabled: 0,
            framebuffer_only: 0,
            pixel_format: WMT_PIXEL_FORMAT_INVALID,
        };
        // SAFETY: a live layer (the caller's promise); the props are written for the call.
        unsafe { MetalLayer_getProps(desc.layer, &raw mut layer_props) };
        layer_props.device = desc.device;
        layer_props.opaque = 1;
        layer_props.display_sync_enabled = 0;
        // Setting it true made presents slower.
        layer_props.framebuffer_only = 0;
        layer_props.contents_scale *= f64::from(desc.scale_factor);
        let mut info = WMTTextureInfo {
            pixel_format: WMT_PIXEL_FORMAT_RGBA32_FLOAT,
            width: 1024,
            height: 1,
            depth: 1,
            array_length: 1,
            bits: (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
                | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
                | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
                | (WMT_TEXTURE_USAGE_SHADER_READ << WMTTextureInfo::USAGE_SHIFT),
            options: WMT_RESOURCE_STORAGE_MODE_SHARED,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        // SAFETY: a live device (the caller's promise); the info is alive for the call.
        let texture = unsafe { MTLDevice_newTexture(desc.device, &raw mut info) };
        Self {
            device: desc.device,
            layer: desc.layer,
            library: desc.library,
            state: Mutex::new(State {
                present_scale: None,
                present_blit: None,
                layer_props,
                sample_count: desc.sample_count,
                colorspace: WMT_COLOR_SPACE_SRGB,
                hdr_metadata: None,
                source_format: WMT_PIXEL_FORMAT_INVALID,
                display_setting_version: 0,
                display_colorspace: WMT_COLOR_SPACE_SRGB,
                display_hdr_metadata: NO_HDR_METADATA,
                display_edr_value: WMTEDRValue {
                    maximum_edr_color_component_value: 0.0,
                    maximum_potential_edr_color_component_value: 1.0,
                },
                gamma_version: 0,
                pipelines_valid: false,
                frame_requested: 0,
            }),
            // SAFETY: `newTexture` answers +1 or null.
            gamma_lut_texture: unsafe { Obj::adopt(texture) },
            presented: Mutex::new(0),
            frame_done: Condvar::new(),
        }
    }

    /// The backbuffer's format, colour space, size and sample count.
    ///
    /// True when the pipelines are rebuilt at the next [`Presenter::synchronize`], else the layer
    /// takes the new size now.
    pub fn change_layer_properties(
        &self,
        format: u32,
        colorspace: u64,
        width: f64,
        height: f64,
        sample_count: u8,
    ) -> bool {
        let mut state = self.state();
        let invalidated = state.colorspace != colorspace
            || state.source_format != format
            || state.sample_count != sample_count;
        state.source_format = format;
        state.sample_count = sample_count;
        state.layer_props.pixel_format = forget_srgb(format);
        state.layer_props.drawable_height = height;
        state.layer_props.drawable_width = width;
        state.colorspace = colorspace;
        if invalidated {
            state.pipelines_valid = false;
        } else {
            // SAFETY: a live layer (`new`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
        }
        invalidated
    }

    /// The swapchain's colour space; true when the pipelines are rebuilt for it.
    pub fn change_layer_color_space(&self, colorspace: u64) -> bool {
        let mut state = self.state();
        let invalidated = state.colorspace != colorspace;
        state.colorspace = colorspace;
        if invalidated {
            state.pipelines_valid = false;
        }
        invalidated
    }

    /// The swapchain's HDR metadata, or none; setting one always rebuilds the pipelines.
    pub fn change_hdr_metadata(&self, metadata: Option<&WMTHDRMetadata>) {
        let mut state = self.state();
        if metadata.is_some() || state.hdr_metadata.is_some() {
            state.pipelines_valid = false;
        }
        state.hdr_metadata = metadata.copied();
    }

    /// The output's gamma ramp, or none.
    ///
    /// A ramp whose version moved is uploaded to the lookup texture and rebuilds the pipelines.
    pub fn change_gamma_ramp(&self, ramp: Option<&DXMTGammaRamp>) {
        let mut state = self.state();
        let Some(ramp) = ramp else {
            if state.gamma_version != 0 {
                state.gamma_version = 0;
                state.pipelines_valid = false;
            }
            return;
        };
        let version = ramp.version.load(Acquire);
        if state.gamma_version == version {
            return;
        }
        state.gamma_version = version;
        let lut = gamma_lut(ramp);
        // SAFETY: the lookup texture or null; `lut` is a 1024×1 RGBA32F row, alive for the call.
        unsafe {
            MTLTexture_replaceRegion(
                self.gamma_lut_texture.as_ref().map_or(0, Obj::handle),
                WMTOrigin { x: 0, y: 0, z: 0 },
                WMTSize {
                    width: 1024,
                    height: 1,
                    depth: 1,
                },
                0,
                0,
                mem(lut.as_ptr()),
                1024 * 16,
                0,
            );
        };
        state.pipelines_valid = false;
    }

    /// The next frame's id and metadata.
    ///
    /// The display's setting is read first and the pipelines rebuilt if something changed.
    ///
    /// A rebuild first waits, holding no lock, until every frame handed out is done
    /// ([`Presenter::frame_done`]).
    pub fn synchronize(&self) -> (u64, DXMTPresentMetadata) {
        let mut guard = self.state();
        let state = &mut *guard;
        let mut version = 0;
        // SAFETY: a live layer (`new`'s promise); each out pointer is a field of `state`, alive
        // for the call (a layer on no screen writes the version only, as the C++ relied on).
        unsafe {
            WMTQueryDisplaySettingForLayer(
                self.layer,
                &raw mut version,
                &raw mut state.display_colorspace,
                &raw mut state.display_hdr_metadata,
                &raw mut state.display_edr_value,
            );
        };
        if version != state.display_setting_version {
            state.display_setting_version = version;
            state.pipelines_valid = false;
        }
        let display = state.display_setting_version > 0;
        let colorspace = if display {
            state.display_colorspace
        } else {
            state.colorspace
        };
        let is_hdr = colorspace & 0b100 != 0;
        let hdr_metadata = if display {
            Some(state.display_hdr_metadata)
        } else {
            state.hdr_metadata
        };
        if !state.pipelines_valid {
            state.pipelines_valid = true;
            let requested = state.frame_requested;
            drop(guard);
            self.wait_presented(requested);
            guard = self.state();
            let state = &mut *guard;
            self.build_pipelines(
                state,
                colorspace == WMT_COLOR_SPACE_HDR_PQ,
                is_hdr && hdr_metadata.is_some(),
            );
            // SAFETY: a live layer; the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
            // SAFETY: a live layer.
            unsafe { MetalLayer_setColorSpace(self.layer, colorspace) };
        }
        let metadata = present_metadata(colorspace, guard.display_edr_value, hdr_metadata);
        guard.frame_requested += 1;
        (guard.frame_requested, metadata)
    }

    /// The frame `frame_id` is done (the C++'s `PresentState` died).
    ///
    /// A rebuild waiting for it goes on. The count never moves back.
    pub fn frame_done(&self, frame_id: u64) {
        let mut presented = self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if frame_id <= *presented {
            return;
        }
        *presented = frame_id;
        drop(presented);
        self.frame_done.notify_all();
    }

    /// Draws `backbuffer` to the layer's next drawable in a render pass of `cmdbuf`.
    ///
    /// Returns the drawable.
    ///
    /// `wait` and `update` get the pass's encoder before and after its commands. The pipelines
    /// are the ones of the last [`Presenter::synchronize`], held until the pass is encoded.
    ///
    /// # Safety
    ///
    /// `cmdbuf` and `backbuffer` are live.
    pub unsafe fn encode(
        &self,
        cmdbuf: u64,
        backbuffer: u64,
        metadata: &DXMTPresentMetadata,
        wait: impl FnOnce(u64),
        update: impl FnOnce(u64),
    ) -> Option<Obj> {
        // An Arc clone, not a Metal retain: a rebuild may replace them while this frame encodes
        // (frame generation's pacing thread encodes frames whose state is gone).
        let (blit, scale, width, height) = {
            let state = self.state();
            (
                state.present_blit.clone(),
                state.present_scale.clone(),
                state.layer_props.drawable_width,
                state.layer_props.drawable_height,
            )
        };
        // SAFETY: a live layer (`new`'s promise).
        let next = unsafe { MetalLayer_nextDrawable(self.layer) };
        // SAFETY: null or the drawable the layer just handed out, autoreleased.
        let drawable = unsafe { Obj::retain(next) };
        // SAFETY: the drawable or null.
        let texture = unsafe { MetalDrawable_texture(drawable.as_ref().map_or(0, Obj::handle)) };
        let mut info = render_pass(texture);
        // SAFETY: a live command buffer (the caller's promise); the info is alive for the call.
        let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut info) };
        wait(encoder);
        // SAFETY: a live texture (the caller's promise).
        let source_width = unsafe { MTLTexture_width(backbuffer) };
        // SAFETY: as above.
        let source_height = unsafe { MTLTexture_height(backbuffer) };
        let matched = same(source_width, width) && same(source_height, height);
        let pipeline = if matched { blit } else { scale };
        let viewport = WMTViewport {
            origin_x: 0.0,
            origin_y: 0.0,
            width,
            height,
            znear: 0.0,
            zfar: 1.0,
        };
        // The records, last first: each names the next.
        let draw = WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: mem(null::<u8>()),
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        };
        let viewports = WmtcmdRenderSetviewports {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
            reserved: [0; 3],
            next: mem(from_ref(&draw)),
            viewports: mem(from_ref(&viewport)),
            viewport_count: 1,
        };
        let pso = WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: mem(from_ref(&viewports)),
            pso: pipeline.as_deref().map_or(0, Obj::handle),
        };
        let bytes = WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: mem(from_ref(&pso)),
            bytes: mem(from_ref(metadata)),
            length: 12,
            index: 0,
        };
        let lut = WmtcmdRenderSettexture {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
            reserved: [0; 3],
            next: mem(from_ref(&bytes)),
            texture: self.gamma_lut_texture.as_ref().map_or(0, Obj::handle),
            index: 1,
        };
        let source = WmtcmdRenderSettexture {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
            reserved: [0; 3],
            next: mem(from_ref(&lut)),
            texture: backbuffer,
            index: 0,
        };
        // SAFETY: the encoder `renderCommandEncoder` answered; the records, each starting with a
        // `wmtcmd_base`, and what they point to are alive for the call.
        unsafe {
            MTLRenderCommandEncoder_encodeCommands(encoder, from_ref(&source).cast::<WmtcmdBase>());
        };
        update(encoder);
        // SAFETY: the same encoder, ended once.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
        drawable
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait_presented(&self, frame_id: u64) {
        let presented = self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        drop(
            self.frame_done
                .wait_while(presented, |presented| *presented < frame_id)
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    /// `buildRenderPipelineState`: both pipelines, with the six function constants.
    fn build_pipelines(&self, state: &mut State, is_pq: bool, with_hdr_metadata: bool) {
        let _pool = Pool::new();
        let mut error = 0;
        {
            let source_is_srgb = forget_srgb(state.source_format) != state.source_format;
            let values = [
                true,
                is_pq,
                with_hdr_metadata,
                source_is_srgb,
                state.sample_count > 1,
                state.gamma_version != 0,
            ];
            let data = values.map(u32::from);
            let mut constants: [WMTFunctionConstant; 6] = core::array::from_fn(|i| {
                WMTFunctionConstant {
                    data: mem(from_ref(&data[i])),
                    r#type: WMT_DATA_TYPE_BOOL,
                    // kPresentFCIndex_*: 0x100 + the position.
                    index: 0x100 + u16::try_from(i).unwrap_or(0),
                    reserved: 0,
                }
            });
            // SAFETY: a live library (`new`'s promise); a NUL-terminated name.
            let vs = unsafe { MTLLibrary_newFunction(self.library, c"vs_present_quad".as_ptr()) };
            // SAFETY: `newFunction` answers +1 or null.
            let vs = unsafe { Obj::adopt(vs) };
            let fs = self.present_function(&constants, &mut error);
            let unmatched = 0_u32;
            constants[0].data = mem(from_ref(&unmatched));
            let fs_scaled = self.present_function(&constants, &mut error);
            let handle = |f: &Option<Obj>| f.as_ref().map_or(0, Obj::handle);
            let mut info = pipeline_info(state.layer_props.pixel_format);
            info.vertex_function = handle(&vs);
            info.fragment_function = handle(&fs);
            state.present_blit = self.pipeline(&info, &mut error);
            info.fragment_function = handle(&fs_scaled);
            state.present_scale = self.pipeline(&info, &mut error);
        }
        // SAFETY: the last error a call wrote, +1, or null (the C++ kept only the last).
        drop(unsafe { Obj::adopt(error) });
    }

    fn present_function(
        &self,
        constants: &[WMTFunctionConstant; 6],
        error: &mut u64,
    ) -> Option<Obj> {
        // SAFETY: a live library; a NUL-terminated name; the six constants and what they point to
        // are alive for the call.
        let function = unsafe {
            MTLLibrary_newFunctionWithConstants(
                self.library,
                c"fs_present_quad".as_ptr(),
                constants.as_ptr(),
                6,
                error,
            )
        };
        // SAFETY: +1 or null.
        unsafe { Obj::adopt(function) }
    }

    fn pipeline(&self, info: &WMTRenderPipelineInfo, error: &mut u64) -> Option<Arc<Obj>> {
        // SAFETY: a live device (`new`'s promise); the info is alive for the call.
        let pso = unsafe { MTLDevice_newRenderPipelineState(self.device, info, error) };
        // SAFETY: +1 or null.
        unsafe { Obj::adopt(pso) }.map(Arc::new)
    }
}

/// What the encoding thread reads, and what the changes write.
struct State {
    // Fields drop in declaration order: the C++ released the scaled pipeline first.
    present_scale: Option<Arc<Obj>>,
    present_blit: Option<Arc<Obj>>,
    layer_props: WMTLayerProps,
    sample_count: u8,
    colorspace: u64,
    hdr_metadata: Option<WMTHDRMetadata>,
    source_format: u32,
    display_setting_version: u64,
    display_colorspace: u64,
    display_hdr_metadata: WMTHDRMetadata,
    display_edr_value: WMTEDRValue,
    gamma_version: u64,
    pipelines_valid: bool,
    frame_requested: u64,
}

const NO_HDR_METADATA: WMTHDRMetadata = WMTHDRMetadata {
    red_primary: [0; 2],
    green_primary: [0; 2],
    blue_primary: [0; 2],
    white_point: [0; 2],
    max_mastering_luminance: 0,
    min_mastering_luminance: 0,
    max_content_light_level: 0,
    max_frame_average_light_level: 0,
};

/// The lookup texture's row: each curve's point clamped to [0, 1] (a NaN stays), alpha 1.
fn gamma_lut(ramp: &DXMTGammaRamp) -> Vec<f32> {
    let point = |curve: &[AtomicU32; GAMMA_CP_COUNT], i: usize| {
        f32::from_bits(curve[i].load(Relaxed)).clamp(0.0, 1.0)
    };
    (0..GAMMA_CP_COUNT)
        .flat_map(|i| {
            [
                point(&ramp.red, i),
                point(&ramp.green, i),
                point(&ramp.blue, i),
                1.0,
            ]
        })
        .collect()
}

/// The present pass's metadata (`synchronizeLayerProperties`).
///
/// For the final colour space, the display's EDR values and the HDR metadata in effect.
fn present_metadata(
    colorspace: u64,
    edr: WMTEDRValue,
    hdr_metadata: Option<WMTHDRMetadata>,
) -> DXMTPresentMetadata {
    let max_display_luminance = edr.maximum_potential_edr_color_component_value * 100.0;
    let mut metadata = DXMTPresentMetadata {
        edr_scale: 1.0,
        max_content_luminance: 10000.0,
        max_display_luminance,
    };
    if colorspace & 0b100 != 0 {
        metadata.edr_scale =
            edr.maximum_edr_color_component_value / edr.maximum_potential_edr_color_component_value;
        if let Some(hdr) = hdr_metadata {
            let max = f32_to_u32(max_display_luminance)
                .max(u32::from(hdr.max_content_light_level))
                .max(hdr.max_mastering_luminance)
                .max(u32::from(hdr.max_frame_average_light_level));
            metadata.max_content_luminance = u32_to_f32(max);
        }
    }
    if colorspace == WMT_COLOR_SPACE_HDR_SC_RGB {
        // A float times the double 0.8, rounded back.
        metadata.edr_scale = f64_to_f32(f64::from(metadata.edr_scale) * 0.8);
    }
    metadata
}

/// `InitializeRenderPassInfo` with the drawable's texture as colour 0, not loaded, stored.
fn render_pass(texture: u64) -> WMTRenderPassInfo {
    // SAFETY: integers, floats and handles, for which all zeros is a value (and zeroed padding,
    // as the C++'s memset).
    let mut info: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    for color in &mut info.colors {
        color.store_action = WMT_STORE_ACTION_STORE;
        color.clear_color.a = 1.0;
    }
    info.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
    info.depth.load_action = WMT_LOAD_ACTION_CLEAR;
    info.depth.clear_depth = 1.0;
    info.colors[0].load_action = WMT_LOAD_ACTION_DONT_CARE;
    info.colors[0].texture = texture;
    info
}

/// `InitializeRenderPipelineInfo` with colour 0 in `format`.
fn pipeline_info(format: u32) -> WMTRenderPipelineInfo {
    let color = WMTColorAttachmentBlendInfo {
        pixel_format: WMT_PIXEL_FORMAT_INVALID,
        rgb_blend_operation: WMT_BLEND_OPERATION_ADD,
        alpha_blend_operation: WMT_BLEND_OPERATION_ADD,
        src_rgb_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_rgb_blend_factor: WMT_BLEND_FACTOR_ZERO,
        src_alpha_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_alpha_blend_factor: WMT_BLEND_FACTOR_ZERO,
        write_mask: WMT_COLOR_WRITE_MASK_ALL,
        blending_enabled: 0,
    };
    let mut info = WMTRenderPipelineInfo {
        colors: [color; 8],
        alpha_to_coverage_enabled: 0,
        logic_operation_enabled: 0,
        logic_operation: WMT_LOGIC_OPERATION_CLEAR,
        rasterization_enabled: 1,
        raster_sample_count: 1,
        depth_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        stencil_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        vertex_function: 0,
        fragment_function: 0,
        immutable_vertex_buffers: 0,
        immutable_fragment_buffers: 0,
        input_primitive_topology: WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED,
        tessellation_partition_mode: WMT_TESSELLATION_PARTITION_MODE_POW2,
        max_tessellation_factor: 16,
        tessellation_output_winding_order: WMT_WINDING_CLOCKWISE,
        tessellation_factor_step: WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT,
        binary_archive_for_serialization: 0,
        binary_archives_for_lookup: mem(null::<u8>()),
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 0,
        padding: [0; 5],
    };
    info.colors[0].pixel_format = format;
    info
}

/// Whether a texture's size is the drawable's: `(uint64_t)size == length`.
///
/// C++'s conversion goes toward zero, so `size` is in `[length, length + 1)`, or in `(-1, 1)` for
/// 0, compared as floats (no `trunc`: on i686 that is a C runtime import).
fn same(length: u64, size: f64) -> bool {
    let length_f = u64_to_f64(length);
    if length == 0 {
        size.abs() < 1.0
    } else {
        (length_f..length_f + 1.0).contains(&size)
    }
}

/// `WMTMemoryPointer` of a pointer: eight bytes, the high half 0 in a 32-bit process.
#[cfg(target_pointer_width = "64")]
fn mem<T>(p: *const T) -> MemPtr {
    MemPtr {
        ptr: p.expose_provenance() as u64,
    }
}

/// `WMTMemoryPointer` of a pointer: eight bytes, the high half 0 in a 32-bit process.
#[cfg(target_pointer_width = "32")]
fn mem<T>(p: *const T) -> MemPtr {
    MemPtr {
        ptr: u32::from_ne_bytes(p.expose_provenance().to_ne_bytes()),
        high: 0,
    }
}

crate::seam_export! {
    /// A presenter, freed by [`dxmt_presenter_destroy`].
    ///
    /// # Safety
    ///
    /// The device, layer and library are live and outlive the presenter.
    unsafe fn dxmt_presenter_create(
        device: u64,
        layer: u64,
        library: u64,
        scale_factor: f32,
        sample_count: u8
    ) -> *mut c_void {
        let desc = PresenterDesc {
            device,
            layer,
            library,
            scale_factor,
            sample_count,
        };
        // SAFETY: the caller's promise.
        let presenter = unsafe { Presenter::new(&desc) };
        Box::into_raw(Box::new(presenter)).cast::<c_void>()
    }
}

crate::seam_export! {
    /// Releases the presenter's pipelines and lookup texture and frees it.
    ///
    /// # Safety
    ///
    /// `p` is a presenter [`dxmt_presenter_create`] returned and nothing has destroyed.
    unsafe fn dxmt_presenter_destroy(p: *mut c_void) {
        // SAFETY: the caller's promise: a live box, given back here.
        drop(unsafe { Box::from_raw(p.cast::<Presenter>()) });
    }
}

crate::seam_export! {
    /// [`Presenter::change_layer_properties`].
    ///
    /// # Safety
    ///
    /// `p` is a live presenter.
    unsafe fn dxmt_presenter_change_layer_properties(
        p: *mut c_void,
        format: u32,
        colorspace: u64,
        width: f64,
        height: f64,
        sample_count: u8
    ) -> bool {
        // SAFETY: the caller's promise.
        let presenter = unsafe { presenter(p) };
        presenter.change_layer_properties(format, colorspace, width, height, sample_count)
    }
}

crate::seam_export! {
    /// [`Presenter::change_layer_color_space`].
    ///
    /// # Safety
    ///
    /// `p` is a live presenter.
    unsafe fn dxmt_presenter_change_layer_color_space(p: *mut c_void, colorspace: u64) -> bool {
        // SAFETY: the caller's promise.
        unsafe { presenter(p) }.change_layer_color_space(colorspace)
    }
}

crate::seam_export! {
    /// [`Presenter::change_hdr_metadata`]; null for none.
    ///
    /// # Safety
    ///
    /// `p` is a live presenter; `metadata` is null or readable.
    unsafe fn dxmt_presenter_change_hdr_metadata(p: *mut c_void, metadata: *const WMTHDRMetadata) {
        // SAFETY: the caller's promise.
        let metadata = unsafe { metadata.as_ref() };
        // SAFETY: the caller's promise.
        unsafe { presenter(p) }.change_hdr_metadata(metadata);
    }
}

crate::seam_export! {
    /// [`Presenter::change_gamma_ramp`]; null for none.
    ///
    /// # Safety
    ///
    /// `p` is a live presenter; `ramp` is null or an output's live gamma ramp.
    unsafe fn dxmt_presenter_change_gamma_ramp(p: *mut c_void, ramp: *const DXMTGammaRamp) {
        // SAFETY: the caller's promise.
        let ramp = unsafe { ramp.as_ref() };
        // SAFETY: the caller's promise.
        unsafe { presenter(p) }.change_gamma_ramp(ramp);
    }
}

crate::seam_export! {
    /// [`Presenter::synchronize`]: the frame's id, its metadata in `*out`.
    ///
    /// # Safety
    ///
    /// `p` is a live presenter; `out` is writable.
    unsafe fn dxmt_presenter_synchronize(p: *mut c_void, out: *mut DXMTPresentMetadata) -> u64 {
        // SAFETY: the caller's promise.
        let (frame_id, metadata) = unsafe { presenter(p) }.synchronize();
        // SAFETY: the caller's promise.
        unsafe { out.write(metadata) };
        frame_id
    }
}

crate::seam_export! {
    /// [`Presenter::frame_done`].
    ///
    /// # Safety
    ///
    /// `p` is a live presenter.
    unsafe fn dxmt_presenter_frame_done(p: *mut c_void, frame_id: u64) {
        // SAFETY: the caller's promise.
        unsafe { presenter(p) }.frame_done(frame_id);
    }
}

crate::seam_export! {
    /// [`Presenter::encode`]: the drawable, +1 (or null).
    ///
    /// # Safety
    ///
    /// `p` is a live presenter; `cmdbuf` and `backbuffer` are live; `metadata` and `callbacks` are
    /// readable, and each callback may be called once with its `ctx` during this call.
    unsafe fn dxmt_presenter_encode(
        p: *mut c_void,
        cmdbuf: u64,
        backbuffer: u64,
        metadata: *const DXMTPresentMetadata,
        callbacks: *const DxmtPresenterCallbacks
    ) -> u64 {
        // SAFETY: the caller's promise.
        let callbacks = unsafe { &*callbacks };
        // SAFETY: the caller's promise.
        let metadata = unsafe { &*metadata };
        // SAFETY: the caller's promise.
        let presenter = unsafe { presenter(p) };
        let wait = |encoder| {
            // SAFETY: the caller's promise: once, during this call, with its `ctx`.
            unsafe { (callbacks.wait)(callbacks.ctx, encoder) };
        };
        let update = |encoder| {
            // SAFETY: as above.
            unsafe { (callbacks.update)(callbacks.ctx, encoder) };
        };
        // SAFETY: the caller's promise for the command buffer and the backbuffer.
        let drawable = unsafe { presenter.encode(cmdbuf, backbuffer, metadata, wait, update) };
        // The reference goes to the caller, so the `Obj` must not release it.
        drawable.map_or(0, |d| ManuallyDrop::new(d).handle())
    }
}

/// The presenter behind a `void *` the C++ holds.
///
/// # Safety
///
/// `p` is a live presenter from [`dxmt_presenter_create`].
const unsafe fn presenter<'a>(p: *mut c_void) -> &'a Presenter {
    // SAFETY: the caller's promise.
    unsafe { &*p.cast::<Presenter>() }
}

#[cfg(test)]
mod tests;
