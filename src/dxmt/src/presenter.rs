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
pub mod framegen;

use core::{
    ffi::c_void,
    mem::ManuallyDrop,
    ptr::{from_ref, null},
    sync::atomic::{
        AtomicU32,
        Ordering::{Acquire, Relaxed},
    },
};
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::Duration,
};

use dxmt_util::{
    GAMMA_CP_COUNT, GammaRamp as DXMTGammaRamp,
    float_cast::{f32_to_u32, f64_to_f32, u32_to_f32, u64_to_f64},
    sync::lock,
};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLCommandBuffer_blitCommandEncoder,
    MTLCommandBuffer_renderCommandEncoder, MTLCommandEncoder_endEncoding,
    MTLDevice_newRenderPipelineState, MTLDevice_newTexture, MTLLibrary_newFunction,
    MTLLibrary_newFunctionWithConstants, MTLRenderCommandEncoder_encodeCommands, MTLTexture_height,
    MTLTexture_pixelFormat, MTLTexture_replaceRegion, MTLTexture_width, MetalDrawable_texture,
    MetalLayer_getEDRValue, MetalLayer_getProps, MetalLayer_nextDrawable, MetalLayer_setColorSpace,
    MetalLayer_setProps, Obj, Pool, WMTQueryDisplaySettingForLayer,
    wire::{
        MemPtr, WMT_BLEND_FACTOR_ONE, WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD,
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_COLOR_SPACE_HDR_PQ,
        WMT_COLOR_SPACE_HDR_SC_RGB, WMT_COLOR_SPACE_SRGB, WMT_COLOR_WRITE_MASK_ALL,
        WMT_DATA_TYPE_BOOL, WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_DONT_CARE,
        WMT_LOGIC_OPERATION_CLEAR, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_INVALID,
        WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMT_PIXEL_FORMAT_RGBA32_FLOAT,
        WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED, WMT_PRIMITIVE_TYPE_TRIANGLE,
        WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_VIEWPORTS, WMT_RESOURCE_STORAGE_MODE_SHARED,
        WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_MULTISAMPLE_RESOLVE, WMT_STORE_ACTION_STORE,
        WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT, WMT_TESSELLATION_PARTITION_MODE_POW2,
        WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_SHADER_READ, WMT_WINDING_CLOCKWISE,
        WMTColorAttachmentBlendInfo, WMTEDRValue, WMTFunctionConstant, WMTHDRMetadata,
        WMTLayerProps, WMTOrigin, WMTRenderPassInfo, WMTRenderPipelineInfo, WMTSize,
        WMTTextureInfo, WMTViewport, WmtcmdBase, WmtcmdBlitCopyFromTextureToTexture,
        WmtcmdRenderDraw, WmtcmdRenderSetbytes, WmtcmdRenderSetpso, WmtcmdRenderSettexture,
        WmtcmdRenderSetviewports,
    },
};

use crate::format::forget_srgb;

/// Per-frame D3D9 presentation parameters carried into the command encoder.
///
/// Snapshot on the API thread so queued frames never race with subsequent ramp or EDR updates.
#[derive(Clone)]
pub struct D3d9PresentArgs {
    /// Packed 256-entry `ushort4` (`[R, G, B, 0xffff]`) gamma lookup table, when active.
    pub gamma: Option<Arc<[u16; 1024]>>,
    /// Effective EDR headroom multiplier (`> 1.0` enables BT.2446-A; `1.0` selects passthrough).
    pub peak_edr: f32,
    /// Attached view handle passed to `MTLDrawable_setPresentedEvent` (`0` when unmanaged).
    pub view: u64,
    /// Per-window `MTLSharedEvent` signaled when presented (`0` disables slot 163).
    pub presented_event: u64,
    /// Monotonic signal value paired with `presented_event`.
    pub presented_value: u64,
    /// Whether this present is paced (`WMT_PRESENTED_EVENT_FLAG_PACED` in slot 163).
    pub paced: bool,
}

/// `DXMTPresentMetadata`: what the present pass's fragment function reads as its bytes.
#[derive(Clone, Copy)]
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
    // Dropped first so a drawable worker joins before the layer or pipelines go away.
    prefetcher: Option<DrawablePrefetcher>,
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

/// Whether the VM host installed WSI hooks (`WMTGetWsiHooks`), enabling drawable prefetch.
#[cfg(any(windows, test))]
const fn should_prefetch_drawables() -> bool {
    false
}

/// Whether the VM host installed WSI hooks (`WMTGetWsiHooks`), enabling drawable prefetch.
#[cfg(all(not(windows), not(test)))]
fn should_prefetch_drawables() -> bool {
    dxmt_util::wsi::hooked()
}

impl Presenter {
    /// The presenter of `desc.layer`.
    ///
    /// # Safety
    ///
    /// `desc`'s device, layer and library are live and outlive the presenter.
    #[must_use]
    pub unsafe fn new(desc: &PresenterDesc) -> Self {
        // SAFETY: the caller's promise.
        unsafe { Self::with_prefetch(desc, should_prefetch_drawables()) }
    }

    /// The presenter of `desc.layer`, optionally prefetching drawables on a worker thread.
    ///
    /// # Safety
    ///
    /// `desc`'s device, layer and library are live and outlive the presenter.
    #[must_use]
    pub unsafe fn with_prefetch(desc: &PresenterDesc, prefetch_drawable: bool) -> Self {
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
        // SAFETY: `newTexture` answers +1 or null.
        let gamma_lut_texture = unsafe { Obj::adopt(texture) };
        let prefetcher = if prefetch_drawable {
            DrawablePrefetcher::new(desc.layer)
        } else {
            None
        };
        Self {
            prefetcher,
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
                fast_blit: false,
                pipelines_valid: false,
                frame_requested: 0,
                d3d9: None,
            }),
            gamma_lut_texture,
            presented: Mutex::new(0),
            frame_done: Condvar::new(),
        }
    }

    /// Constructs a D3D9 presenter on `desc.layer` with initial `width` x `height` drawable size.
    ///
    /// Skips allocating the unused 1024x1 `gamma_lut_texture` and leaves layer properties ready
    /// for [`Presenter::prepare_d3d9_present`] and [`Presenter::set_d3d9_drawable_size`].
    ///
    /// # Safety
    ///
    /// `desc`'s device, layer and library are live and outlive the presenter.
    #[must_use]
    pub unsafe fn new_d3d9(
        desc: &PresenterDesc,
        width: u32,
        height: u32,
        hdr_enable: bool,
    ) -> Arc<Self> {
        // SAFETY: the caller's promise.
        unsafe { Self::new_d3d9_with_mode(desc, width, height, hdr_enable, false) }
    }

    /// Builds a D3D9 presenter whose `CAMetalLayer` properties belong to `WMTAttachMetalView`.
    ///
    /// Reads existing properties with `MetalLayer_getProps` and never invokes
    /// `MetalLayer_setProps`, `MetalLayer_setColorSpace`, or `MetalLayer_getEDRValue`.
    ///
    /// # Safety
    ///
    /// `desc`'s device, layer and library are live and outlive the presenter.
    #[must_use]
    pub unsafe fn new_d3d9_managed(
        desc: &PresenterDesc,
        width: u32,
        height: u32,
        hdr_enable: bool,
    ) -> Arc<Self> {
        // SAFETY: the caller's promise.
        unsafe { Self::new_d3d9_with_mode(desc, width, height, hdr_enable, true) }
    }

    #[must_use]
    unsafe fn new_d3d9_with_mode(
        desc: &PresenterDesc,
        width: u32,
        height: u32,
        hdr_enable: bool,
        managed_layer: bool,
    ) -> Arc<Self> {
        debug_assert!(desc.sample_count <= 1);
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
        if managed_layer {
            // `WMTAttachMetalView` owns the layer with `framebufferOnly = true`.
            layer_props.framebuffer_only = 1;
            if layer_props.drawable_width <= 0.0 || layer_props.drawable_height <= 0.0 {
                layer_props.drawable_width = f64::from(width);
                layer_props.drawable_height = f64::from(height);
            }
            if layer_props.pixel_format == WMT_PIXEL_FORMAT_INVALID {
                layer_props.pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
            }
        } else {
            layer_props.opaque = 1;
            layer_props.display_sync_enabled = 1;
            layer_props.framebuffer_only = 0;
            layer_props.contents_scale *= f64::from(desc.scale_factor);
            layer_props.drawable_width = f64::from(width);
            layer_props.drawable_height = f64::from(height);
            layer_props.pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
            // SAFETY: a live layer (the caller's promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(desc.layer, &raw const layer_props) };
            // SAFETY: a live layer (the caller's promise).
            unsafe { MetalLayer_setColorSpace(desc.layer, WMT_COLOR_SPACE_SRGB) };
        }
        let init_mode =
            if managed_layer && layer_props.pixel_format == WMT_PIXEL_FORMAT_RGBA16_FLOAT {
                D3d9LayerMode::Hdr
            } else {
                D3d9LayerMode::Sdr
            };
        let init_colorspace = match init_mode {
            D3d9LayerMode::Hdr => WMT_COLOR_SPACE_HDR_SC_RGB,
            D3d9LayerMode::Sdr => WMT_COLOR_SPACE_SRGB,
        };
        let prefetcher = if should_prefetch_drawables() {
            DrawablePrefetcher::new(desc.layer)
        } else {
            None
        };
        Arc::new(Self {
            prefetcher,
            device: desc.device,
            layer: desc.layer,
            library: desc.library,
            state: Mutex::new(State {
                present_scale: None,
                present_blit: None,
                layer_props,
                sample_count: desc.sample_count,
                colorspace: init_colorspace,
                hdr_metadata: None,
                source_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                display_setting_version: 0,
                display_colorspace: WMT_COLOR_SPACE_SRGB,
                display_hdr_metadata: NO_HDR_METADATA,
                display_edr_value: WMTEDRValue {
                    maximum_edr_color_component_value: 0.0,
                    maximum_potential_edr_color_component_value: 1.0,
                },
                gamma_version: 0,
                fast_blit: false,
                pipelines_valid: false,
                frame_requested: 0,
                d3d9: Some(D3d9State {
                    hdr_enable,
                    mode: init_mode,
                    display_sync: true,
                    managed_layer,
                    edr_poll_countdown: 0,
                    last_edr: (1.0, 1.0),
                    psos: FxHashMap::default(),
                }),
            }),
            gamma_lut_texture: None,
            presented: Mutex::new(0),
            frame_done: Condvar::new(),
        })
    }

    /// Updates cached D3D9 layer assumptions from `WMTViewSyncAndPoll` without mutating the layer.
    ///
    /// Invalidates cached drawables cleanly whenever `colorspace_changed`, HDR mode, or drawable
    /// dimensions moved.
    pub fn sync_d3d9_managed(
        &self,
        width: u32,
        height: u32,
        hdr_active: bool,
        colorspace_changed: bool,
    ) {
        let w = f64::from(width);
        let h = f64::from(height);
        let mut guard = self.state();
        let state = &mut *guard;
        let Some(d3d9) = state.d3d9.as_mut() else {
            return;
        };
        let want_mode = if d3d9.hdr_enable && hdr_active {
            D3d9LayerMode::Hdr
        } else {
            D3d9LayerMode::Sdr
        };
        let mode_changed = d3d9.mode != want_mode;
        let size_changed = state.layer_props.drawable_width.to_bits() != w.to_bits()
            || state.layer_props.drawable_height.to_bits() != h.to_bits();
        if !colorspace_changed && !mode_changed && !size_changed {
            return;
        }
        d3d9.mode = want_mode;
        state.layer_props.drawable_width = w;
        state.layer_props.drawable_height = h;
        state.layer_props.pixel_format = if want_mode == D3d9LayerMode::Hdr {
            WMT_PIXEL_FORMAT_RGBA16_FLOAT
        } else {
            WMT_PIXEL_FORMAT_BGRA8_UNORM
        };
        state.colorspace = if want_mode == D3d9LayerMode::Hdr {
            WMT_COLOR_SPACE_HDR_SC_RGB
        } else {
            WMT_COLOR_SPACE_SRGB
        };
        if colorspace_changed || mode_changed {
            state.pipelines_valid = false;
        }
        drop(guard);
        self.invalidate_drawable();
    }

    /// Synchronizes D3D9 layer EDR mode, drawable size, and vsync on the API thread.
    ///
    /// Polls `MetalLayer_getEDRValue` at most once every 32 frames when `hdr_enable` is set,
    /// invokes `on_mode_change` before SDR/HDR transitions, updates `CAMetalLayer` only when mode,
    /// size, or `display_sync` changes, and returns the effective peak EDR multiplier
    /// (`> 1.0` for BT.2446-A).
    pub fn prepare_d3d9_present(
        &self,
        width: u32,
        height: u32,
        display_sync: bool,
        on_mode_change: impl FnOnce(),
    ) -> f32 {
        let Some((managed_hdr, should_poll, cached_edr)) = self.state().d3d9.as_ref().map(|d3d9| {
            if d3d9.managed_layer {
                (Some(d3d9.mode == D3d9LayerMode::Hdr), false, (1.0, 1.0))
            } else if d3d9.hdr_enable {
                (None, d3d9.edr_poll_countdown == 0, d3d9.last_edr)
            } else {
                (None, false, (1.0, 1.0))
            }
        }) else {
            return 1.0;
        };
        if let Some(hdr_active) = managed_hdr {
            self.sync_d3d9_managed(width, height, hdr_active, false);
            return 1.0;
        }
        let (cur, pot) = if should_poll {
            let _pool = Pool::new();
            let mut edr = WMTEDRValue {
                maximum_edr_color_component_value: 1.0,
                maximum_potential_edr_color_component_value: 1.0,
            };
            // SAFETY: `self.layer` is live for the presenter's lifetime; `edr` is writable.
            unsafe { MetalLayer_getEDRValue(self.layer, &raw mut edr) };
            (
                edr.maximum_edr_color_component_value,
                edr.maximum_potential_edr_color_component_value,
            )
        } else {
            cached_edr
        };
        let w = f64::from(width);
        let h = f64::from(height);
        let mut guard = self.state();
        let state = &mut *guard;
        let Some(d3d9) = state.d3d9.as_mut() else {
            return 1.0;
        };
        if should_poll {
            d3d9.last_edr = (cur, pot);
            d3d9.edr_poll_countdown = D3D9_EDR_POLL_INTERVAL - 1;
        } else if d3d9.hdr_enable {
            d3d9.edr_poll_countdown = d3d9.edr_poll_countdown.saturating_sub(1);
        }
        let want_hdr = d3d9.hdr_enable && pot > 1.0 && pot.is_finite();
        let want_mode = if want_hdr {
            D3d9LayerMode::Hdr
        } else {
            D3d9LayerMode::Sdr
        };
        let mode_changed = d3d9.mode != want_mode;
        let props_changed = state.layer_props.drawable_width.to_bits() != w.to_bits()
            || state.layer_props.drawable_height.to_bits() != h.to_bits()
            || d3d9.display_sync != display_sync;
        if mode_changed {
            drop(guard);
            on_mode_change();
            self.invalidate_drawable();
            guard = self.state();
            let state = &mut *guard;
            let Some(d3d9) = state.d3d9.as_mut() else {
                return 1.0;
            };
            d3d9.mode = want_mode;
            d3d9.display_sync = display_sync;
            if want_hdr {
                d3d9.edr_poll_countdown = 0;
            }
            state.layer_props.drawable_width = w;
            state.layer_props.drawable_height = h;
            state.layer_props.display_sync_enabled = u8::from(display_sync);
            state.layer_props.framebuffer_only = u8::from(want_hdr);
            state.layer_props.pixel_format = if want_hdr {
                WMT_PIXEL_FORMAT_RGBA16_FLOAT
            } else {
                WMT_PIXEL_FORMAT_BGRA8_UNORM
            };
            state.colorspace = if want_hdr {
                WMT_COLOR_SPACE_HDR_SC_RGB
            } else {
                WMT_COLOR_SPACE_SRGB
            };
            if want_hdr {
                // SAFETY: a live layer (`new_d3d9`'s promise); the props are alive for the call.
                unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
                // SAFETY: a live layer (`new_d3d9`'s promise).
                unsafe { MetalLayer_setColorSpace(self.layer, WMT_COLOR_SPACE_HDR_SC_RGB) };
            } else {
                // SAFETY: a live layer (`new_d3d9`'s promise).
                unsafe { MetalLayer_setColorSpace(self.layer, WMT_COLOR_SPACE_SRGB) };
                // SAFETY: a live layer (`new_d3d9`'s promise); the props are alive for the call.
                unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
            }
            drop(guard);
            self.invalidate_drawable();
        } else if props_changed {
            d3d9.display_sync = display_sync;
            state.layer_props.drawable_width = w;
            state.layer_props.drawable_height = h;
            state.layer_props.display_sync_enabled = u8::from(display_sync);
            // SAFETY: a live layer (`new_d3d9`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
            drop(guard);
            self.invalidate_drawable();
        } else {
            drop(guard);
        }
        if want_hdr && cur > 1.0 && cur.is_finite() {
            cur
        } else {
            1.0
        }
    }

    /// Updates the D3D9 layer's drawable size while preserving its current HDR or SDR mode.
    pub fn set_d3d9_drawable_size(&self, width: u32, height: u32) {
        let w = f64::from(width);
        let h = f64::from(height);
        let mut state = self.state();
        let managed = state.d3d9.as_ref().is_some_and(|d3d9| d3d9.managed_layer);
        if let Some(d3d9) = state.d3d9.as_mut() {
            d3d9.edr_poll_countdown = 0;
        }
        if state.layer_props.drawable_width.to_bits() == w.to_bits()
            && state.layer_props.drawable_height.to_bits() == h.to_bits()
        {
            return;
        }
        state.layer_props.drawable_width = w;
        state.layer_props.drawable_height = h;
        if !managed {
            // SAFETY: a live layer (`new`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
        }
        drop(state);
        self.invalidate_drawable();
    }

    /// The backbuffer's format, colour space, size and sample count (`changeLayerProperties`).
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
        state.layer_props.pixel_format = desired_layer_format(&state);
        state.layer_props.drawable_height = height;
        state.layer_props.drawable_width = width;
        state.colorspace = colorspace;
        if invalidated {
            state.pipelines_valid = false;
        } else {
            // SAFETY: a live layer (`new`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
        }
        drop(state);
        self.invalidate_drawable();
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
        drop(state);
        self.invalidate_drawable();
        invalidated
    }

    /// The swapchain's HDR metadata, or none; setting one always rebuilds the pipelines.
    pub fn change_hdr_metadata(&self, metadata: Option<&WMTHDRMetadata>) {
        let mut state = self.state();
        if metadata.is_some() || state.hdr_metadata.is_some() {
            state.pipelines_valid = false;
        }
        state.hdr_metadata = metadata.copied();
        let desired = desired_layer_format(&state);
        if desired != state.layer_props.pixel_format {
            state.layer_props.pixel_format = desired;
            // SAFETY: a live layer (`new`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
            drop(state);
            self.invalidate_drawable();
        }
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
        state.layer_props.pixel_format = desired_layer_format(&state);
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
                MemPtr::from_ptr(lut.as_ptr()),
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
        let desired = desired_layer_format_for(state, is_hdr || hdr_metadata.is_some());
        if desired != state.layer_props.pixel_format {
            state.layer_props.pixel_format = desired;
            // SAFETY: a live layer (`new`'s promise); the props are alive for the call.
            unsafe { MetalLayer_setProps(self.layer, &raw const state.layer_props) };
            self.invalidate_drawable();
        }
        state.fast_blit = state.gamma_version == 0 && !is_hdr && hdr_metadata.is_none();
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
            self.invalidate_drawable();
        }
        let metadata = present_metadata(colorspace, guard.display_edr_value, hdr_metadata);
        guard.frame_requested += 1;
        (guard.frame_requested, metadata)
    }

    /// Drops a prefetched drawable after a layer property or colour-space change.
    fn invalidate_drawable(&self) {
        if let Some(prefetcher) = &self.prefetcher {
            prefetcher.invalidate();
        }
    }

    /// The frame `frame_id` is done (the C++'s `PresentState` died).
    ///
    /// A rebuild waiting for it goes on. The count never moves back.
    pub fn frame_done(&self, frame_id: u64) {
        let mut presented = lock(&self.presented);
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
        let drawable = self.acquire_drawable();
        // SAFETY: the caller's promises, forwarded unchanged.
        unsafe { self.encode_with_drawable(drawable, cmdbuf, backbuffer, metadata, wait, update) }
    }

    /// Takes the next drawable for a present, without encoding anything yet.
    ///
    /// A caller that must wait for a queue lock or a drawable before it can encode can acquire
    /// here first, so the wait does not hold that lock.
    #[must_use]
    pub fn acquire_drawable(&self) -> Option<Obj> {
        self.prefetcher.as_ref().map_or_else(
            || {
                // SAFETY: a live layer (`new`'s promise).
                let next = unsafe { MetalLayer_nextDrawable(self.layer) };
                // SAFETY: null or the drawable the layer just handed out, autoreleased.
                unsafe { Obj::retain(next) }
            },
            DrawablePrefetcher::acquire,
        )
    }

    /// Draws `backbuffer` into an already-acquired `drawable`.
    ///
    /// `drawable` is `acquire_drawable`'s answer. Passing it separately lets the caller acquire
    /// before taking a queue lock; `None` encodes against a null drawable, as [`Self::encode`]
    /// does when the layer has none.
    ///
    /// # Safety
    ///
    /// `cmdbuf` and `backbuffer` are live.
    pub unsafe fn encode_with_drawable(
        &self,
        drawable: Option<Obj>,
        cmdbuf: u64,
        backbuffer: u64,
        metadata: &DXMTPresentMetadata,
        wait: impl FnOnce(u64),
        update: impl FnOnce(u64),
    ) -> Option<Obj> {
        // An Arc clone, not a Metal retain: a rebuild may replace them while this frame encodes
        // (frame generation's pacing thread encodes frames whose state is gone).
        let (blit, scale, width, height, fast_blit, sample_count) = {
            let state = self.state();
            (
                state.present_blit.clone(),
                state.present_scale.clone(),
                state.layer_props.drawable_width,
                state.layer_props.drawable_height,
                state.fast_blit,
                state.sample_count,
            )
        };
        // SAFETY: the drawable or null.
        let texture = unsafe { MetalDrawable_texture(drawable.as_ref().map_or(0, Obj::handle)) };
        // SAFETY: a live texture (the caller's promise).
        let source_width = unsafe { MTLTexture_width(backbuffer) };
        // SAFETY: as above.
        let source_height = unsafe { MTLTexture_height(backbuffer) };
        let matched = same(source_width, width) && same(source_height, height);
        let can_blit = if fast_blit
            && sample_count <= 1
            && matched
            && texture != 0
            && backbuffer != 0
            && width > 0.0
            && height > 0.0
        {
            // SAFETY: `backbuffer` is a live texture (the caller's promise).
            let source_format = unsafe { MTLTexture_pixelFormat(backbuffer) };
            // SAFETY: `texture` is the drawable's texture, or 0, which the format query accepts.
            let drawable_format = unsafe { MTLTexture_pixelFormat(texture) };
            // A blit neither converts sRGB nor changes a format. Equal formats mean the
            // drawable already interprets the source's encoded values the right way.
            source_format == drawable_format
        } else {
            false
        };
        let can_resolve =
            if fast_blit && sample_count > 1 && matched && texture != 0 && backbuffer != 0 {
                // SAFETY: `backbuffer` is a live texture (the caller's promise).
                let source_format = unsafe { MTLTexture_pixelFormat(backbuffer) };
                // SAFETY: `texture` is the drawable's texture, or 0.
                let drawable_format = unsafe { MTLTexture_pixelFormat(texture) };
                source_format == drawable_format
            } else {
                false
            };
        if can_resolve {
            let mut info = resolve_pass(backbuffer, texture, sample_count);
            // SAFETY: a live command buffer; `info` is alive for the call.
            let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut info) };
            wait(encoder);
            update(encoder);
            // SAFETY: the same encoder, ended once; its store action performs the resolve.
            unsafe { MTLCommandEncoder_endEncoding(encoder) };
            return drawable;
        }
        if can_blit {
            // SAFETY: the caller's promises for `cmdbuf` and `backbuffer`; `texture` is live.
            unsafe {
                encode_blit_present(
                    cmdbuf,
                    backbuffer,
                    texture,
                    (source_width, source_height),
                    wait,
                    update,
                );
            }
            return drawable;
        }
        let mut info = render_pass(texture);
        // SAFETY: a live command buffer (the caller's promise); the info is alive for the call.
        let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut info) };
        wait(encoder);
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
            next: MemPtr::from_ptr(null::<u8>()),
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        };
        let viewports = WmtcmdRenderSetviewports {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
            reserved: [0; 3],
            next: MemPtr::from_ptr(from_ref(&draw)),
            viewports: MemPtr::from_ptr(from_ref(&viewport)),
            viewport_count: 1,
        };
        let pso = WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: MemPtr::from_ptr(from_ref(&viewports)),
            pso: pipeline.as_deref().map_or(0, Obj::handle),
        };
        let bytes = WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: MemPtr::from_ptr(from_ref(&pso)),
            bytes: MemPtr::from_ptr(from_ref(metadata)),
            length: 12,
            index: 0,
        };
        let lut = WmtcmdRenderSettexture {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
            reserved: [0; 3],
            next: MemPtr::from_ptr(from_ref(&bytes)),
            texture: self.gamma_lut_texture.as_ref().map_or(0, Obj::handle),
            index: 1,
        };
        let source = WmtcmdRenderSettexture {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
            reserved: [0; 3],
            next: MemPtr::from_ptr(from_ref(&lut)),
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

    /// Draws `backbuffer` to the layer's next drawable using D3D9 per-frame gamma and EDR state.
    ///
    /// # Safety
    ///
    /// `cmdbuf` and `backbuffer` are live.
    pub unsafe fn encode_d3d9(
        &self,
        cmdbuf: u64,
        backbuffer: u64,
        args: &D3d9PresentArgs,
        wait: impl FnOnce(u64),
        update: impl FnOnce(u64),
    ) -> Option<Obj> {
        let drawable = self.acquire_drawable();
        // SAFETY: the drawable or null.
        let texture = unsafe { MetalDrawable_texture(drawable.as_ref().map_or(0, Obj::handle)) };
        if texture == 0 || backbuffer == 0 {
            return None;
        }
        // SAFETY: `texture` is a live drawable texture (checked non-zero above).
        let tex_w = unsafe { MTLTexture_width(texture) };
        // SAFETY: as above.
        let tex_h = unsafe { MTLTexture_height(texture) };
        let (layer_width, layer_height, layer_format, sample_count, framebuffer_only) = {
            let state = self.state();
            (
                state.layer_props.drawable_width,
                state.layer_props.drawable_height,
                state.layer_props.pixel_format,
                state.sample_count,
                state.layer_props.framebuffer_only != 0,
            )
        };
        let (draw_width, draw_height) = if tex_w > 0 && tex_h > 0 {
            u32::try_from(tex_w)
                .ok()
                .zip(u32::try_from(tex_h).ok())
                .map_or((layer_width, layer_height), |(tw, th)| {
                    (f64::from(tw), f64::from(th))
                })
        } else {
            (layer_width, layer_height)
        };
        // SAFETY: `texture` is a live drawable texture (checked non-zero above).
        let raw_fmt = unsafe { MTLTexture_pixelFormat(texture) };
        let drawable_fmt = if raw_fmt == 0 { layer_format } else { raw_fmt };
        // SAFETY: `backbuffer` is a live texture (the caller's promise).
        let source_width = unsafe { MTLTexture_width(backbuffer) };
        // SAFETY: as above.
        let source_height = unsafe { MTLTexture_height(backbuffer) };
        let can_blit = if !framebuffer_only
            && args.gamma.is_none()
            && sample_count <= 1
            && drawable_fmt == WMT_PIXEL_FORMAT_BGRA8_UNORM
            && source_width > 0
            && source_height > 0
            && same(source_width, draw_width)
            && same(source_height, draw_height)
        {
            // SAFETY: `backbuffer` is a live texture (the caller's promise).
            let source_format = unsafe { MTLTexture_pixelFormat(backbuffer) };
            source_format == drawable_fmt
        } else {
            false
        };
        if can_blit {
            // SAFETY: the caller's promises for `cmdbuf` and `backbuffer`; `texture` is live.
            unsafe {
                encode_blit_present(
                    cmdbuf,
                    backbuffer,
                    texture,
                    (source_width, source_height),
                    wait,
                    update,
                );
            }
            return drawable;
        }
        let has_gamma = args.gamma.is_some();
        let has_hdr_headroom = args.peak_edr > 1.0 && args.peak_edr.is_finite();
        let variant = if drawable_fmt == WMT_PIXEL_FORMAT_RGBA16_FLOAT {
            if has_hdr_headroom {
                if has_gamma {
                    D3d9PresentVariant::HdrBt2446Gamma
                } else {
                    D3d9PresentVariant::HdrBt2446
                }
            } else if has_gamma {
                D3d9PresentVariant::HdrPassthroughGamma
            } else {
                D3d9PresentVariant::HdrPassthrough
            }
        } else if has_gamma {
            D3d9PresentVariant::SdrGamma
        } else {
            D3d9PresentVariant::Sdr
        };
        // SAFETY: `cmdbuf`, `backbuffer`, and `texture` are live Metal objects.
        unsafe {
            self.encode_d3d9_quad(
                cmdbuf,
                &D3d9Quad {
                    src: backbuffer,
                    dst: texture,
                    draw_size: (draw_width, draw_height),
                    target_fmt: drawable_fmt,
                    variant,
                    gamma: args.gamma.as_deref(),
                    peak_edr: args.peak_edr,
                },
                wait,
                update,
            );
        }
        drawable
    }

    /// Encodes a fullscreen D3D9 present/gamma quad described by `quad`.
    ///
    /// # Safety
    ///
    /// `cmdbuf`, `quad.src`, and `quad.dst` are live Metal objects.
    unsafe fn encode_d3d9_quad(
        &self,
        cmdbuf: u64,
        quad: &D3d9Quad<'_>,
        wait: impl FnOnce(u64),
        update: impl FnOnce(u64),
    ) {
        let &D3d9Quad {
            src,
            dst,
            draw_size: (draw_width, draw_height),
            target_fmt,
            variant,
            gamma,
            peak_edr,
        } = quad;
        let pipeline = self.d3d9_pso(variant, target_fmt);
        let mut info = render_pass(dst);
        if pipeline.is_none() {
            info.colors[0].load_action = WMT_LOAD_ACTION_CLEAR;
        }
        // SAFETY: a live command buffer (the caller's promise); `info` is alive for the call.
        let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut info) };
        wait(encoder);
        if let Some(pipeline) = pipeline.as_deref() {
            let viewport = WMTViewport {
                origin_x: 0.0,
                origin_y: 0.0,
                width: draw_width,
                height: draw_height,
                znear: 0.0,
                zfar: 1.0,
            };
            let uniforms = variant
                .uses_hdr_uniforms()
                .then(|| d3d9_hdr_uniforms(peak_edr));
            let draw = WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: MemPtr::from_ptr(null::<u8>()),
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            };
            let viewports = WmtcmdRenderSetviewports {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                reserved: [0; 3],
                next: MemPtr::from_ptr(from_ref(&draw)),
                viewports: MemPtr::from_ptr(from_ref(&viewport)),
                viewport_count: 1,
            };
            let pso = WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: MemPtr::from_ptr(from_ref(&viewports)),
                pso: pipeline.handle(),
            };
            let mut head = from_ref(&pso).cast::<WmtcmdBase>();
            let gamma_cmd =
                gamma
                    .filter(|_| variant.uses_gamma())
                    .map(|lut| WmtcmdRenderSetbytes {
                        r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
                        reserved: [0; 3],
                        next: MemPtr::from_ptr(head),
                        bytes: MemPtr::from_ptr(lut.as_ptr()),
                        length: 2048,
                        index: 1,
                    });
            if let Some(cmd) = &gamma_cmd {
                head = from_ref(cmd).cast::<WmtcmdBase>();
            }
            let uniforms_cmd = uniforms.as_ref().map(|u| WmtcmdRenderSetbytes {
                r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
                reserved: [0; 3],
                next: MemPtr::from_ptr(head),
                bytes: MemPtr::from_ptr(u),
                length: 16,
                index: 0,
            });
            if let Some(cmd) = &uniforms_cmd {
                head = from_ref(cmd).cast::<WmtcmdBase>();
            }
            let source = WmtcmdRenderSettexture {
                r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
                reserved: [0; 3],
                next: MemPtr::from_ptr(head),
                texture: src,
                index: 0,
            };
            // SAFETY: the encoder `renderCommandEncoder` answered; the records and what they point
            // to are alive for the call.
            unsafe {
                MTLRenderCommandEncoder_encodeCommands(
                    encoder,
                    from_ref(&source).cast::<WmtcmdBase>(),
                );
            };
        }
        update(encoder);
        // SAFETY: the same encoder, ended once.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    fn d3d9_pso(&self, variant: D3d9PresentVariant, format: u32) -> Option<Arc<Obj>> {
        let key = (variant, format);
        {
            let state = self.state();
            if let Some(cached) = state.d3d9.as_ref().and_then(|d| d.psos.get(&key)) {
                return cached.clone();
            }
        }
        let built = {
            let _pool = Pool::new();
            let mut error = 0;
            // SAFETY: a live library (`new_d3d9`'s promise); a NUL-terminated name.
            let vs = unsafe { MTLLibrary_newFunction(self.library, c"vs_present_quad".as_ptr()) };
            // SAFETY: `newFunction` answers +1 or null.
            let vs = unsafe { Obj::adopt(vs) };
            // SAFETY: a live library (`new_d3d9`'s promise); a NUL-terminated name.
            let fs =
                unsafe { MTLLibrary_newFunction(self.library, variant.entry_point().as_ptr()) };
            // SAFETY: `newFunction` answers +1 or null.
            let fs = unsafe { Obj::adopt(fs) };
            let pso = match (vs.as_ref(), fs.as_ref()) {
                (Some(vs), Some(fs)) => {
                    let mut info = pipeline_info(format);
                    info.vertex_function = vs.handle();
                    info.fragment_function = fs.handle();
                    self.pipeline(&info, &mut error)
                }
                _ => None,
            };
            // SAFETY: the last error a call wrote, +1, or null.
            let err_obj = unsafe { Obj::adopt(error) };
            if pso.is_none() {
                let detail = match (vs.as_ref(), fs.as_ref(), err_obj.as_ref()) {
                    (None, _, _) => "missing vertex function vs_present_quad".to_owned(),
                    (_, None, _) => format!(
                        "missing fragment function {}",
                        variant.entry_point().to_string_lossy()
                    ),
                    (Some(_), Some(_), Some(err)) => crate::library::description(err),
                    (Some(_), Some(_), None) => "unknown Metal error".to_owned(),
                };
                crate::log::err(&format!(
                    "d3d9 presenter: failed to create pipeline {variant:?} (format={format:#x}): {detail}"
                ));
            }
            drop(err_obj);
            pso
        };
        let mut state = self.state();
        let (kept, discarded) = if let Some(d3d9) = state.d3d9.as_mut() {
            if let Some(existing) = d3d9.psos.get(&key) {
                (existing.clone(), built)
            } else {
                d3d9.psos.insert(key, built.clone());
                (built, None)
            }
        } else {
            (built, None)
        };
        drop(state);
        drop(discarded);
        kept
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    fn wait_presented(&self, frame_id: u64) {
        let presented = lock(&self.presented);
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
            let drawable_is_srgb = is_srgb_format(state.layer_props.pixel_format);
            let values = [
                true,
                is_pq,
                with_hdr_metadata,
                source_is_srgb && !drawable_is_srgb,
                state.sample_count > 1,
                state.gamma_version != 0,
            ];
            let data = values.map(u32::from);
            let mut constants: [WMTFunctionConstant; 6] = core::array::from_fn(|i| {
                WMTFunctionConstant {
                    data: MemPtr::from_ptr(from_ref(&data[i])),
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
            constants[0].data = MemPtr::from_ptr(from_ref(&unmatched));
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

/// Background worker that prefetches the next `CAMetalLayer` drawable for the VM host.
struct DrawablePrefetcher {
    shared: Arc<DrawableShared>,
    thread: Option<JoinHandle<()>>,
}

struct DrawableShared {
    state: Mutex<DrawableState>,
    cv: Condvar,
}

struct DrawableState {
    stop: bool,
    wanted: bool,
    busy: bool,
    generation: u64,
    prefetched_generation: u64,
    prefetched: Option<Obj>,
}

impl DrawablePrefetcher {
    fn new(layer: u64) -> Option<Self> {
        let shared = Arc::new(DrawableShared {
            state: Mutex::new(DrawableState {
                stop: false,
                wanted: false,
                busy: false,
                generation: 0,
                prefetched_generation: 0,
                prefetched: None,
            }),
            cv: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .spawn(move || drawable_worker(layer, &worker_shared))
            .ok()?;
        Some(Self {
            shared,
            thread: Some(thread),
        })
    }

    fn request_locked(state: &mut DrawableState, cv: &Condvar) {
        if state.stop || state.wanted || state.busy || state.prefetched.is_some() {
            return;
        }
        state.wanted = true;
        cv.notify_all();
    }

    fn invalidate(&self) {
        let discarded = {
            let mut state = lock(&self.shared.state);
            state.generation += 1;
            state.prefetched.take()
        };
        drop(discarded);
        self.shared.cv.notify_all();
    }

    fn acquire(&self) -> Option<Obj> {
        loop {
            let mut state = lock(&self.shared.state);
            if state.stop {
                return None;
            }
            if state.prefetched.is_some() && state.prefetched_generation == state.generation {
                let drawable = state.prefetched.take();
                Self::request_locked(&mut state, &self.shared.cv);
                return drawable;
            }
            let discarded = state.prefetched.take();
            Self::request_locked(&mut state, &self.shared.cv);
            drop(discarded);
            state = self
                .shared
                .cv
                .wait_while(state, |s| s.prefetched.is_none() && !s.stop)
                .unwrap_or_else(PoisonError::into_inner);
            drop(state);
        }
    }
}

impl Drop for DrawablePrefetcher {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let discarded = {
                let mut state = lock(&self.shared.state);
                state.stop = true;
                state.prefetched.take()
            };
            drop(discarded);
            self.shared.cv.notify_all();
            while !thread.is_finished() {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn drawable_worker(layer: u64, shared: &DrawableShared) {
    loop {
        let generation = {
            let state = lock(&shared.state);
            let mut state = shared
                .cv
                .wait_while(state, |s| !s.stop && !s.wanted)
                .unwrap_or_else(PoisonError::into_inner);
            if state.stop {
                return;
            }
            state.wanted = false;
            state.busy = true;
            state.generation
        };
        let drawable = {
            let _pool = Pool::new();
            // SAFETY: `layer` is live until `Presenter` drops, which waits for this worker first.
            let next = unsafe { MetalLayer_nextDrawable(layer) };
            // SAFETY: null or the drawable the layer just handed out, autoreleased in `_pool`.
            unsafe { Obj::retain(next) }
        };
        if drawable.is_none() {
            thread::sleep(Duration::from_millis(1));
        }
        let mut discarded = None;
        let mut state = lock(&shared.state);
        state.busy = false;
        if !state.stop && generation == state.generation && state.prefetched.is_none() {
            state.prefetched = drawable;
            state.prefetched_generation = generation;
        } else {
            discarded = drawable;
        }
        drop(state);
        drop(discarded);
        shared.cv.notify_all();
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
    /// No gamma/HDR/MSAA work is needed and a direct texture copy can present this frame.
    fast_blit: bool,
    pipelines_valid: bool,
    frame_requested: u64,
    /// Opt-in D3D9 presentation state, `None` for D3D11/D3D12 presenters.
    d3d9: Option<D3d9State>,
}

const D3D9_EDR_POLL_INTERVAL: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum D3d9LayerMode {
    Sdr,
    Hdr,
}

struct D3d9Quad<'a> {
    src: u64,
    dst: u64,
    draw_size: (f64, f64),
    target_fmt: u32,
    variant: D3d9PresentVariant,
    gamma: Option<&'a [u16; 1024]>,
    peak_edr: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum D3d9PresentVariant {
    Sdr,
    SdrGamma,
    HdrPassthrough,
    HdrPassthroughGamma,
    HdrBt2446,
    HdrBt2446Gamma,
}

impl D3d9PresentVariant {
    const fn entry_point(self) -> &'static core::ffi::CStr {
        match self {
            Self::Sdr => c"fs_present_d3d9_sdr",
            Self::SdrGamma => c"fs_present_d3d9_sdr_gamma",
            Self::HdrPassthrough => c"fs_present_d3d9_hdr_passthrough",
            Self::HdrPassthroughGamma => c"fs_present_d3d9_hdr_passthrough_gamma",
            Self::HdrBt2446 => c"fs_present_d3d9_hdr_bt2446",
            Self::HdrBt2446Gamma => c"fs_present_d3d9_hdr_bt2446_gamma",
        }
    }

    const fn uses_hdr_uniforms(self) -> bool {
        matches!(self, Self::HdrBt2446 | Self::HdrBt2446Gamma)
    }

    const fn uses_gamma(self) -> bool {
        matches!(
            self,
            Self::SdrGamma | Self::HdrPassthroughGamma | Self::HdrBt2446Gamma
        )
    }
}

/// BT.2408's HDR reference white in nits: where the HDR picture puts the screen's SDR white.
///
/// Matches `dxmt_command.metal`'s `REFERENCE_WHITE_NITS`, so 203 nits show at EDR `1.0`.
const REFERENCE_WHITE_NITS: f32 = 203.0;

/// Top of PQ and of BT.2446, in nits.
const PQ_PEAK_NITS: f32 = 10000.0;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct D3D9HdrUniforms {
    l_hdr_nits: f32,
    p_hdr: f32,
    log2_p_hdr: f32,
    inv_p_minus_one: f32,
}

const _: () = assert!(size_of::<[u16; 1024]>() == 2048);
const _: () = assert!(size_of::<D3D9HdrUniforms>() == 16);

fn d3d9_hdr_uniforms(peak: f32) -> D3D9HdrUniforms {
    let l_hdr_nits = (peak.max(1.0) * REFERENCE_WHITE_NITS).min(PQ_PEAK_NITS);
    let p_hdr = 32.0_f32.mul_add((l_hdr_nits / PQ_PEAK_NITS).powf(1.0 / 2.4), 1.0);
    let log2_p_hdr = p_hdr.log2();
    let inv_p_minus_one = 1.0 / (p_hdr - 1.0);
    D3D9HdrUniforms {
        l_hdr_nits,
        p_hdr,
        log2_p_hdr,
        inv_p_minus_one,
    }
}

struct D3d9State {
    hdr_enable: bool,
    mode: D3d9LayerMode,
    display_sync: bool,
    managed_layer: bool,
    edr_poll_countdown: u32,
    last_edr: (f32, f32),
    psos: FxHashMap<(D3d9PresentVariant, u32), Option<Arc<Obj>>>,
}

/// Encodes a direct blit present between `wait` and `update` fence passes.
///
/// # Safety
///
/// `cmdbuf`, `backbuffer`, and `texture` are live Metal objects.
unsafe fn encode_blit_present(
    cmdbuf: u64,
    backbuffer: u64,
    texture: u64,
    size: (u64, u64),
    wait: impl FnOnce(u64),
    update: impl FnOnce(u64),
) {
    // Keep the fence callbacks on a render encoder, but make it a no-draw, no-store
    // pass, and put the actual copy on a blit encoder.
    let mut before = fence_pass(texture);
    // SAFETY: a live command buffer; `before` is alive for the call.
    let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut before) };
    wait(encoder);
    // SAFETY: the same encoder, ended once.
    unsafe { MTLCommandEncoder_endEncoding(encoder) };

    // SAFETY: `cmdbuf` is a live command buffer.
    let blit_encoder = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf) };
    let copy = WmtcmdBlitCopyFromTextureToTexture {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
        reserved: [0; 3],
        next: MemPtr::from_ptr(null::<u8>()),
        src: backbuffer,
        src_slice: 0,
        src_level: 0,
        src_origin: WMTOrigin { x: 0, y: 0, z: 0 },
        src_size: WMTSize {
            width: size.0,
            height: size.1,
            depth: 1,
        },
        dst: texture,
        dst_slice: 0,
        dst_level: 0,
        dst_origin: WMTOrigin { x: 0, y: 0, z: 0 },
    };
    // SAFETY: `blit_encoder` is live and `copy` is a valid record head.
    unsafe {
        MTLBlitCommandEncoder_encodeCommands(blit_encoder, (&raw const copy).cast::<WmtcmdBase>());
    }
    // SAFETY: the same encoder, ended once.
    unsafe { MTLCommandEncoder_endEncoding(blit_encoder) };

    let mut after = fence_pass(texture);
    // SAFETY: a live command buffer; `after` is alive for the call.
    let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut after) };
    update(encoder);
    // SAFETY: the same encoder, ended once.
    unsafe { MTLCommandEncoder_endEncoding(encoder) };
}

/// The layer format a state needs.
///
/// The source's own encoding for SDR without a gamma LUT, and the non-sRGB view whenever a
/// shader must convert or tone-map.
const fn desired_layer_format(state: &State) -> u32 {
    desired_layer_format_for(state, false)
}

/// The same answer, with the display's HDR state forced on or off.
const fn desired_layer_format_for(state: &State, display_hdr: bool) -> u32 {
    if !display_hdr && state.gamma_version == 0 && state.hdr_metadata.is_none() {
        state.source_format
    } else {
        forget_srgb(state.source_format)
    }
}

/// Whether `format` is an sRGB pixel format.
const fn is_srgb_format(format: u32) -> bool {
    forget_srgb(format) != format
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

/// A render pass that only exists to carry fence commands.
///
/// It has the drawable as its attachment, loads and stores nothing, and draws nothing; a blit
/// between two of these keeps the old wait/update callbacks without a full-screen fragment pass.
fn fence_pass(texture: u64) -> WMTRenderPassInfo {
    // SAFETY: integers, floats and handles, for which all zeros is a value (and zeroed padding).
    let mut info: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    for color in &mut info.colors {
        color.load_action = WMT_LOAD_ACTION_DONT_CARE;
        color.store_action = WMT_STORE_ACTION_DONT_CARE;
    }
    info.colors[0].texture = texture;
    info
}

/// A pass that resolves a multisampled source into the drawable without drawing anything.
fn resolve_pass(source: u64, drawable: u64, sample_count: u8) -> WMTRenderPassInfo {
    // SAFETY: integers, floats and handles, for which all zeros is a value (and zeroed padding).
    let mut info: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    for color in &mut info.colors {
        color.load_action = WMT_LOAD_ACTION_DONT_CARE;
        color.store_action = WMT_STORE_ACTION_DONT_CARE;
    }
    info.default_raster_sample_count = sample_count;
    info.colors[0].texture = source;
    info.colors[0].load_action = WMT_LOAD_ACTION_DONT_CARE;
    info.colors[0].store_action = WMT_STORE_ACTION_MULTISAMPLE_RESOLVE;
    info.colors[0].resolve_texture = drawable;
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
        binary_archives_for_lookup: MemPtr::from_ptr(null::<u8>()),
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
