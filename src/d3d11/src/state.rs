//! D3D11 blend, depth-stencil, rasterizer and sampler state objects (`d3d11_state_object.cpp`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Arc, LazyLock, Mutex};

use dxmt_core::{library::DepthStencilCache, sampler::Sampler as CoreSampler, state_map};
use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
    sync::lock,
};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    Obj,
    wire::{
        WMT_STENCIL_OPERATION_KEEP, WMTColorAttachmentBlendInfo, WMTDepthStencilInfo,
        WMTRenderPipelineBlendInfo, WMTSamplerInfo, WmtcmdRenderSetrasterizerstate,
    },
};

use crate::{
    abi::{
        IID_ID3D10_BLEND_STATE, IID_ID3D10_BLEND_STATE_1, IID_ID3D10_DEPTH_STENCIL_STATE,
        IID_ID3D10_DEVICE_CHILD, IID_ID3D10_RASTERIZER_STATE, IID_ID3D10_SAMPLER_STATE,
        IID_ID3D11_BLEND_STATE, IID_ID3D11_BLEND_STATE_1, IID_ID3D11_DEPTH_STENCIL_STATE,
        IID_ID3D11_DEVICE_CHILD, IID_ID3D11_RASTERIZER_STATE, IID_ID3D11_RASTERIZER_STATE_1,
        IID_ID3D11_RASTERIZER_STATE_2, IID_ID3D11_SAMPLER_STATE, IID_IUNKNOWN,
        records::{
            D3d11BlendDesc, D3d11BlendDesc1, D3d11DepthStencilDesc, D3d11DepthStencilopDesc,
            D3d11RasterizerDesc, D3d11RasterizerDesc1, D3d11RasterizerDesc2,
            D3d11RenderTargetBlendDesc, D3d11RenderTargetBlendDesc1, D3d11SamplerDesc,
        },
        vtbl::{
            ID3D11BlendState1Vtbl, ID3D11DepthStencilStateVtbl, ID3D11RasterizerState2Vtbl,
            ID3D11SamplerStateVtbl,
        },
    },
    com::{E_INVALIDARG, Guid, Hresult, S_FALSE, S_OK, add_ref},
    device::Device,
};

pub mod cache;

use self::cache::{
    CanonicalBlendKey, CanonicalDepthStencilKey, CanonicalRasterizerKey, CanonicalSamplerKey,
    DeviceChildStateObject, StateCaches,
};

/// Next synthetic Metal handle when no live `MTLDevice` is attached to `device`.
static NEXT_SYNTHETIC_HANDLE: AtomicU64 = AtomicU64::new(1);

fn next_synthetic_handle() -> u64 {
    NEXT_SYNTHETIC_HANDLE.fetch_add(1, Ordering::Relaxed)
}

/// Fallback per-device `StateCaches` table when `Device::from_iface(device)` returns `None`.
static FALLBACK_CACHES: LazyLock<Mutex<FxHashMap<usize, Arc<StateCaches>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

fn caches_for_device(device: *mut c_void) -> (&'static StateCaches, Option<&'static Obj>) {
    if let Some(dev) = Device::from_iface(device) {
        let mtl = (dev.core().device().handle() > 0x1000).then(|| dev.core().device());
        return (dev.caches(), mtl);
    }
    let arc = lock(&FALLBACK_CACHES)
        .entry(device as usize)
        .or_insert_with(|| Arc::new(StateCaches::default()))
        .clone();
    let ptr = Arc::as_ptr(&arc);
    // Keep one Arc reference alive in the fallback table for the lifetime of the process or test.
    // SAFETY: `arc` remains stored in `FALLBACK_CACHES` and is never removed.
    (unsafe { &*ptr }, None)
}

/// D3D11 `D3D11_LOGIC_OP` (`0..=15`) to `WMTLogicOperation` (`d3d11_state_object.cpp`).
pub const LOGIC_OP_MAP: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

const fn is_dual_source_factor(factor: u32) -> bool {
    factor >= 16
}

/// One past `D3D11_BLEND`'s last value: 20 and 21 are D3D12's, which D3D11 answers as 0.
const D3D11_BLEND_END: u32 = 20;

/// A render target's blend factors and operations for [`state_map::fill_blend`].
const fn rt_blend(rt: &D3d11RenderTargetBlendDesc1) -> state_map::RtBlend {
    state_map::RtBlend {
        src: rt.src_blend,
        dest: rt.dest_blend,
        op: rt.blend_op,
        src_alpha: rt.src_blend_alpha,
        dest_alpha: rt.dest_blend_alpha,
        op_alpha: rt.blend_op_alpha,
    }
}

/// Promotes a D3D11 `D3D11_BLEND_DESC` into `D3D11_BLEND_DESC1`.
#[must_use]
pub const fn promote_blend_desc(desc: &D3d11BlendDesc) -> D3d11BlendDesc1 {
    let empty = D3d11RenderTargetBlendDesc1 {
        blend_enable: 0,
        logic_op_enable: 0,
        src_blend: 2,
        dest_blend: 1,
        blend_op: 1,
        src_blend_alpha: 2,
        dest_blend_alpha: 1,
        blend_op_alpha: 1,
        logic_op: 4,
        render_target_write_mask: 0x0f,
    };
    let mut rts = [empty; 8];
    let mut i = 0;
    while i < 8 {
        let s = desc.render_target[i];
        rts[i] = D3d11RenderTargetBlendDesc1 {
            blend_enable: s.blend_enable,
            logic_op_enable: 0,
            src_blend: s.src_blend,
            dest_blend: s.dest_blend,
            blend_op: s.blend_op,
            src_blend_alpha: s.src_blend_alpha,
            dest_blend_alpha: s.dest_blend_alpha,
            blend_op_alpha: s.blend_op_alpha,
            logic_op: 4,
            render_target_write_mask: s.render_target_write_mask,
        };
        i += 1;
    }
    D3d11BlendDesc1 {
        alpha_to_coverage_enable: desc.alpha_to_coverage_enable,
        independent_blend_enable: desc.independent_blend_enable,
        render_target: rts,
    }
}

const fn demote_blend_desc(desc: &D3d11BlendDesc1) -> D3d11BlendDesc {
    let empty = D3d11RenderTargetBlendDesc {
        blend_enable: 0,
        src_blend: 2,
        dest_blend: 1,
        blend_op: 1,
        src_blend_alpha: 2,
        dest_blend_alpha: 1,
        blend_op_alpha: 1,
        render_target_write_mask: 0x0f,
    };
    let mut rts = [empty; 8];
    let mut i = 0;
    while i < 8 {
        let s = desc.render_target[i];
        rts[i] = D3d11RenderTargetBlendDesc {
            blend_enable: s.blend_enable,
            src_blend: s.src_blend,
            dest_blend: s.dest_blend,
            blend_op: s.blend_op,
            src_blend_alpha: s.src_blend_alpha,
            dest_blend_alpha: s.dest_blend_alpha,
            blend_op_alpha: s.blend_op_alpha,
            render_target_write_mask: s.render_target_write_mask,
        };
        i += 1;
    }
    D3d11BlendDesc {
        alpha_to_coverage_enable: desc.alpha_to_coverage_enable,
        independent_blend_enable: desc.independent_blend_enable,
        render_target: rts,
    }
}

/// Validates and translates `desc` into a [`BlendState`].
///
/// # Errors
///
/// Returns `E_INVALIDARG` when any active render target enables both blending and logic ops.
pub fn translate_blend_state(desc: &D3d11BlendDesc1) -> Result<BlendState, Hresult> {
    let num_targets = if desc.independent_blend_enable != 0 {
        8
    } else {
        1
    };
    for rt in desc.render_target.iter().take(num_targets) {
        if rt.logic_op_enable != 0 && rt.blend_enable != 0 {
            return Err(E_INVALIDARG);
        }
    }

    let mut desc_normalized = *desc;
    desc_normalized.alpha_to_coverage_enable = i32::from(desc.alpha_to_coverage_enable != 0);
    desc_normalized.independent_blend_enable = i32::from(desc.independent_blend_enable != 0);

    let zero_color = WMTColorAttachmentBlendInfo {
        pixel_format: 0,
        rgb_blend_operation: 0,
        alpha_blend_operation: 0,
        src_rgb_blend_factor: 0,
        dst_rgb_blend_factor: 0,
        src_alpha_blend_factor: 0,
        dst_alpha_blend_factor: 0,
        write_mask: 0,
        blending_enabled: 0,
    };
    let logic_op_enabled = desc_normalized.independent_blend_enable == 0
        && desc_normalized.render_target[0].logic_op_enable != 0;
    let mut info = WMTRenderPipelineBlendInfo {
        colors: [zero_color; 8],
        alpha_to_coverage_enabled: u8::from(desc_normalized.alpha_to_coverage_enable != 0),
        logic_operation_enabled: u8::from(logic_op_enabled),
        logic_operation: if logic_op_enabled {
            LOGIC_OP_MAP
                .get(desc_normalized.render_target[0].logic_op as usize)
                .copied()
                .unwrap_or(0)
        } else {
            0
        },
    };
    for rt in 0..8 {
        let s = if desc_normalized.independent_blend_enable != 0 {
            &desc_normalized.render_target[rt]
        } else {
            &desc_normalized.render_target[0]
        };
        let d = &mut info.colors[rt];
        d.blending_enabled = u8::from(s.blend_enable != 0);
        d.write_mask =
            state_map::color_write_mask(u32::from(s.render_target_write_mask & 0x0f)).unwrap_or(0);
        state_map::fill_blend(d, &rt_blend(s), D3D11_BLEND_END);
    }

    let rt0 = &desc_normalized.render_target[0];
    let dual_source = rt0.blend_enable != 0
        && (is_dual_source_factor(rt0.src_blend)
            || is_dual_source_factor(rt0.src_blend_alpha)
            || is_dual_source_factor(rt0.dest_blend_alpha)
            || is_dual_source_factor(rt0.dest_blend));

    Ok(BlendState {
        hash: blend_desc_hash(&desc_normalized),
        desc: desc_normalized,
        info,
        dual_source,
    })
}

/// The xxh3 of a blend description's fields, the graphics pipeline key's part for it.
///
/// Hashing the description, rather than the object's address, keeps the key valid if an allocator
/// reuses the address and lets two equivalent states share a pipeline. The fields are copied into
/// zeroed storage first: hashing the original record would include its padding bytes, which the
/// application's structure need not define.
fn blend_desc_hash(blend: &D3d11BlendDesc1) -> u64 {
    // SAFETY: `D3d11BlendDesc1` is a POD record of integers for which all zero bits are a valid
    // value.
    let mut desc = unsafe { core::mem::zeroed::<D3d11BlendDesc1>() };
    desc.alpha_to_coverage_enable = blend.alpha_to_coverage_enable;
    desc.independent_blend_enable = blend.independent_blend_enable;
    for (dst, src) in desc.render_target.iter_mut().zip(&blend.render_target) {
        *dst = *src;
    }
    // SAFETY: `desc` is a plain record of exactly this many bytes, fully initialized.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const desc).cast::<u8>(),
            core::mem::size_of::<D3d11BlendDesc1>(),
        )
    };
    xxhash_rust::xxh3::xxh3_64(bytes)
}

/// Compiled `ID3D11BlendState1` (`MTLD3D11BlendState`).
pub struct BlendState {
    pub desc: D3d11BlendDesc1,
    info: WMTRenderPipelineBlendInfo,
    dual_source: bool,
    /// [`blend_desc_hash`] of `desc`, which never changes after creation.
    hash: u64,
}

impl BlendState {
    /// Recovers `&BlendState` from an `ID3D11BlendState*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        let obj = iface.cast::<DeviceChildStateObject<ID3D11BlendState1Vtbl, Self>>();
        // SAFETY: `iface` is a valid `ID3D11BlendState1` pointer created by `StateCaches::blend`.
        Some(unsafe { &(*obj).inner })
    }

    /// Returns the Metal blend attachment descriptor (`info`).
    #[must_use]
    pub const fn info(&self) -> &WMTRenderPipelineBlendInfo {
        &self.info
    }

    /// The xxh3 of the description the graphics pipeline key holds, made at creation.
    #[must_use]
    pub const fn hash(&self) -> u64 {
        self.hash
    }

    /// Returns `true` when dual-source blending is enabled (`IsDualSourceBlending`).
    #[must_use]
    pub const fn is_dual_source_blending(&self) -> bool {
        self.dual_source
    }

    /// Populates `blend_info` for `num_rt` render targets and `valid_rt` mask.
    pub fn setup_metal_pipeline_descriptor(
        &self,
        blend_info: &mut WMTRenderPipelineBlendInfo,
        num_rt: u32,
        valid_rt: u32,
    ) {
        let count = (num_rt as usize).min(8);
        for (rt, attachment_desc) in blend_info.colors.iter_mut().enumerate().take(count) {
            if (valid_rt & (1 << rt)) == 0 {
                continue;
            }
            let i = if self.desc.independent_blend_enable != 0 {
                rt
            } else {
                0
            };
            let render_target = &self.desc.render_target[i];
            attachment_desc.write_mask = state_map::color_write_mask(u32::from(
                render_target.render_target_write_mask & 0x0f,
            ))
            .unwrap_or(0);
            if render_target.blend_enable != 0 && attachment_desc.pixel_format != 0 {
                if dxmt_core::format::is_integer(attachment_desc.pixel_format) {
                    continue;
                }
                attachment_desc.blending_enabled = u8::from(render_target.blend_enable != 0);
                state_map::fill_blend(attachment_desc, &rt_blend(render_target), D3D11_BLEND_END);
            }
        }
        if self.desc.independent_blend_enable == 0
            && self.desc.render_target[0].logic_op_enable != 0
        {
            blend_info.logic_operation_enabled = 1;
            blend_info.logic_operation = LOGIC_OP_MAP
                .get(self.desc.render_target[0].logic_op as usize)
                .copied()
                .unwrap_or(0);
        }
        blend_info.alpha_to_coverage_enabled = u8::from(self.desc.alpha_to_coverage_enable != 0);
    }
}

/// Promotes a D3D11 `D3D11_RASTERIZER_DESC` into `D3D11_RASTERIZER_DESC2`.
#[must_use]
pub const fn promote_rasterizer_desc(desc: &D3d11RasterizerDesc) -> D3d11RasterizerDesc2 {
    D3d11RasterizerDesc2 {
        fill_mode: desc.fill_mode,
        cull_mode: desc.cull_mode,
        front_counter_clockwise: desc.front_counter_clockwise,
        depth_bias: desc.depth_bias,
        depth_bias_clamp: desc.depth_bias_clamp,
        slope_scaled_depth_bias: desc.slope_scaled_depth_bias,
        depth_clip_enable: desc.depth_clip_enable,
        scissor_enable: desc.scissor_enable,
        multisample_enable: desc.multisample_enable,
        antialiased_line_enable: desc.antialiased_line_enable,
        forced_sample_count: 0,
        conservative_raster: 0,
    }
}

/// Promotes a D3D11 `D3D11_RASTERIZER_DESC1` into `D3D11_RASTERIZER_DESC2`.
#[must_use]
pub const fn promote_rasterizer_desc1(desc: &D3d11RasterizerDesc1) -> D3d11RasterizerDesc2 {
    D3d11RasterizerDesc2 {
        fill_mode: desc.fill_mode,
        cull_mode: desc.cull_mode,
        front_counter_clockwise: desc.front_counter_clockwise,
        depth_bias: desc.depth_bias,
        depth_bias_clamp: desc.depth_bias_clamp,
        slope_scaled_depth_bias: desc.slope_scaled_depth_bias,
        depth_clip_enable: desc.depth_clip_enable,
        scissor_enable: desc.scissor_enable,
        multisample_enable: desc.multisample_enable,
        antialiased_line_enable: desc.antialiased_line_enable,
        forced_sample_count: desc.forced_sample_count,
        conservative_raster: 0,
    }
}

/// Validates and translates `desc` into a [`RasterizerState`].
///
/// # Errors
///
/// Returns `E_INVALIDARG` on invalid rasterizer state parameters.
pub const fn translate_rasterizer_state(
    desc: &D3d11RasterizerDesc2,
) -> Result<RasterizerState, Hresult> {
    let fill_mode = if desc.fill_mode == 2 { 1 } else { 0 };
    let cull_mode = match desc.cull_mode {
        2 => 1,
        3 => 2,
        _ => 0,
    };
    let depth_clip_mode = if desc.depth_clip_enable != 0 { 0 } else { 1 };
    let winding = if desc.front_counter_clockwise != 0 {
        1
    } else {
        0
    };
    let depth_bias = dxmt_util::float_cast::i32_to_f32(desc.depth_bias);
    let uav_only_sample_count = if desc.forced_sample_count == 0 {
        1
    } else {
        desc.forced_sample_count
    };
    Ok(RasterizerState {
        desc: *desc,
        fill_mode,
        cull_mode,
        depth_clip_mode,
        winding,
        depth_bias,
        slope_scale: desc.slope_scaled_depth_bias,
        depth_bias_clamp: desc.depth_bias_clamp,
        scissor_enabled: desc.scissor_enable != 0,
        uav_only_sample_count,
    })
}

/// Compiled `ID3D11RasterizerState2` (`MTLD3D11RasterizerState`).
pub struct RasterizerState {
    pub desc: D3d11RasterizerDesc2,
    pub fill_mode: u8,
    pub cull_mode: u8,
    pub depth_clip_mode: u8,
    pub winding: u8,
    pub depth_bias: f32,
    pub slope_scale: f32,
    pub depth_bias_clamp: f32,
    scissor_enabled: bool,
    uav_only_sample_count: u32,
}

impl RasterizerState {
    /// Recovers `&RasterizerState` from an `ID3D11RasterizerState*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        let obj = iface.cast::<DeviceChildStateObject<ID3D11RasterizerState2Vtbl, Self>>();
        // SAFETY: `iface` is a valid `ID3D11RasterizerState2` pointer created by `StateCaches::rasterizer`.
        Some(unsafe { &(*obj).inner })
    }

    /// Populates `cmd` with this rasterizer state (`SetupRasterizerState`).
    pub const fn setup(&self, cmd: &mut WmtcmdRenderSetrasterizerstate) {
        cmd.fill_mode = self.fill_mode;
        cmd.cull_mode = self.cull_mode;
        cmd.depth_clip_mode = self.depth_clip_mode;
        cmd.winding = self.winding;
        cmd.depth_bias = self.depth_bias;
        cmd.scole_scale = self.slope_scale;
        cmd.depth_bias_clamp = self.depth_bias_clamp;
    }

    /// Returns `true` when scissor testing is enabled (`IsScissorEnabled`).
    #[must_use]
    pub const fn is_scissor_enabled(&self) -> bool {
        self.scissor_enabled
    }

    /// Forced sample count for UAV-only rendering (`UAVOnlySampleCount`).
    #[must_use]
    pub const fn uav_only_sample_count(&self) -> u32 {
        self.uav_only_sample_count
    }
}

/// Validates and translates `desc` into a [`DepthStencilState`].
///
/// # Errors
///
/// Returns `E_INVALIDARG` on invalid depth-stencil state parameters.
pub fn translate_depth_stencil_state(
    desc: &D3d11DepthStencilDesc,
    metal: Option<(&Obj, &DepthStencilCache)>,
) -> Result<DepthStencilState, Hresult> {
    let face = |f: &D3d11DepthStencilopDesc| state_map::StencilFace {
        fail_op: f.stencil_fail_op,
        depth_fail_op: f.stencil_depth_fail_op,
        pass_op: f.stencil_pass_op,
        func: f.stencil_func,
    };
    let mut info = state_map::depth_stencil_info(
        &state_map::DepthStencilDesc {
            depth_enable: desc.depth_enable != 0,
            depth_write: desc.depth_write_mask == 1,
            depth_func: desc.depth_func,
            stencil_enable: desc.stencil_enable != 0,
            read_mask: desc.stencil_read_mask,
            write_mask: desc.stencil_write_mask,
            front: face(&desc.front_face),
            back: face(&desc.back_face),
        },
        WMT_STENCIL_OPERATION_KEEP,
    );

    let mut objs = Vec::with_capacity(4);
    let mut make_state = |info: &WMTDepthStencilInfo| -> u64 {
        if let Some((dev, cache)) = metal
            && let Some(obj) = cache.get(dev, info)
        {
            let h = obj.handle();
            objs.push(obj);
            return h;
        }
        next_synthetic_handle()
    };

    let info_default = info;
    let h_default = make_state(&info_default);
    let mut created_count = 1u32;

    let (handles, infos) = if desc.stencil_enable != 0 {
        info.front_stencil.enabled = 0;
        info.back_stencil.enabled = 0;
        let info_stencil_disabled = info;
        let h_stencil_disabled = make_state(&info_stencil_disabled);
        created_count += 1;
        if desc.depth_enable != 0 {
            info.depth_compare_function = 7;
            info.depth_write_enabled = 0;
            let info_both_disabled = info;
            let h_both_disabled = make_state(&info_both_disabled);
            created_count += 1;
            (
                [h_both_disabled, h_stencil_disabled, h_default, h_default],
                [
                    info_both_disabled,
                    info_stencil_disabled,
                    info_default,
                    info_default,
                ],
            )
        } else {
            (
                [h_stencil_disabled, h_stencil_disabled, h_default, h_default],
                [
                    info_stencil_disabled,
                    info_stencil_disabled,
                    info_default,
                    info_default,
                ],
            )
        }
    } else if desc.depth_enable != 0 {
        info.depth_compare_function = 7;
        info.depth_write_enabled = 0;
        let info_depth_disabled = info;
        let h_depth_disabled = make_state(&info_depth_disabled);
        created_count += 1;
        (
            [h_depth_disabled, h_default, h_default, h_default],
            [
                info_depth_disabled,
                info_default,
                info_default,
                info_default,
            ],
        )
    } else {
        (
            [h_default, h_default, h_default, h_default],
            [info_default, info_default, info_default, info_default],
        )
    };

    let h0 = handles[0];
    let rel_handles = [
        u32::try_from(handles[0].wrapping_sub(h0) & 0xffff_ffff).unwrap_or(0),
        u32::try_from(handles[1].wrapping_sub(h0) & 0xffff_ffff).unwrap_or(0),
        u32::try_from(handles[2].wrapping_sub(h0) & 0xffff_ffff).unwrap_or(0),
        u32::try_from(handles[3].wrapping_sub(h0) & 0xffff_ffff).unwrap_or(0),
    ];

    Ok(DepthStencilState {
        desc: *desc,
        handles,
        infos,
        rel_handles,
        mtl_states_created: created_count,
        enabled: desc.depth_enable != 0 || desc.stencil_enable != 0,
        _objs: objs,
    })
}

/// Compiled `ID3D11DepthStencilState` (`MTLD3D11DepthStencilState`).
pub struct DepthStencilState {
    pub desc: D3d11DepthStencilDesc,
    handles: [u64; 4],
    pub infos: [WMTDepthStencilInfo; 4],
    pub rel_handles: [u32; 4],
    pub mtl_states_created: u32,
    enabled: bool,
    _objs: Vec<Obj>,
}

impl DepthStencilState {
    /// Recovers `&DepthStencilState` from an `ID3D11DepthStencilState*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        let obj = iface.cast::<DeviceChildStateObject<ID3D11DepthStencilStateVtbl, Self>>();
        // SAFETY: `iface` is a valid `ID3D11DepthStencilState` pointer created by `StateCaches::depth_stencil`.
        Some(unsafe { &(*obj).inner })
    }

    /// Returns the Metal depth-stencil state handle for `planar` mask (`0..4`).
    #[must_use]
    pub const fn state(&self, planar: u32) -> u64 {
        self.handles[(planar & 3) as usize]
    }

    /// Returns `true` when either depth testing or stencil testing is enabled (`IsEnabled`).
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Validates and translates `desc` into a [`SamplerState`].
///
/// # Errors
///
/// Propagates any error creating the underlying Metal sampler state.
pub fn translate_sampler_state(
    desc: &D3d11SamplerDesc,
    metal_device: Option<&Obj>,
) -> Result<SamplerState, Hresult> {
    let mut primary_info = WMTSamplerInfo {
        min_filter: 0,
        mag_filter: 0,
        mip_filter: 0,
        r_address_mode: 0,
        s_address_mode: 0,
        t_address_mode: 0,
        border_color: 0,
        compare_function: 0,
        lod_min_clamp: 0.0,
        lod_max_clamp: 0.0,
        max_anisotroy: 0,
        normalized_coords: 0,
        lod_average: 0,
        support_argument_buffers: 0,
        gpu_resource_id: 0,
    };
    // D3D11 reads any filter with the 0x80 bit as comparing; what D3D12 warns of passes silently.
    dxmt_core::sampler::populate(
        &mut primary_info,
        &dxmt_core::sampler::SamplerDesc {
            filter: desc.filter,
            address: [desc.address_u, desc.address_v, desc.address_w],
            comparison: desc.filter & 0x80 != 0,
            comparison_func: desc.comparison_func,
            max_anisotropy: desc.max_anisotropy,
            min_lod: desc.min_lod,
            max_lod: desc.max_lod,
            border_color: Some(desc.border_color),
        },
        &mut |_| {},
    );
    let cube_info = dxmt_core::sampler::cube_info(&primary_info);

    let (handle, argument, handle_cube, argument_cube, core_sampler) = if let Some(dev) =
        metal_device
        && let Some(sampler) = CoreSampler::create(dev, &primary_info, desc.mip_lod_bias)
    {
        (
            sampler.state().handle(),
            sampler.gpu_resource_id(),
            sampler.state_cube().handle(),
            sampler.gpu_resource_id_cube(),
            Some(sampler),
        )
    } else {
        let h1 = next_synthetic_handle();
        let h2 = next_synthetic_handle();
        (h1, h1, h2, h2, None)
    };

    Ok(SamplerState {
        desc: *desc,
        handle,
        argument,
        handle_cube,
        argument_cube,
        lod_bias: desc.mip_lod_bias,
        primary_info,
        cube_info,
        core: core_sampler,
    })
}

/// Compiled `ID3D11SamplerState` (`MTLD3D11SamplerState`).
pub struct SamplerState {
    pub desc: D3d11SamplerDesc,
    pub handle: u64,
    pub argument: u64,
    pub handle_cube: u64,
    pub argument_cube: u64,
    pub lod_bias: f32,
    pub primary_info: WMTSamplerInfo,
    pub cube_info: WMTSamplerInfo,
    core: Option<Arc<CoreSampler>>,
}

impl SamplerState {
    /// Recovers `&SamplerState` from an `ID3D11SamplerState*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        let obj = iface.cast::<DeviceChildStateObject<ID3D11SamplerStateVtbl, Self>>();
        // SAFETY: `iface` is a valid `ID3D11SamplerState` pointer created by `StateCaches::sampler`.
        Some(unsafe { &(*obj).inner })
    }

    /// Converts this sampler state into an encoder [`dxmt_core::encoder::BoundSampler`].
    #[must_use]
    pub fn to_bound_sampler(&self) -> dxmt_core::encoder::BoundSampler {
        let owner: Arc<dyn Send + Sync> = match self.core.as_ref() {
            Some(c) => Arc::clone(c) as Arc<dyn Send + Sync>,
            None => Arc::new(()),
        };
        dxmt_core::encoder::BoundSampler {
            state: self.argument,
            cube_state: self.argument_cube,
            lod_bias: self.lod_bias,
            owner,
        }
    }
}

unsafe extern "system-unwind" fn state_add_ref<V: 'static, T>(this: *mut c_void) -> u32 {
    let obj = this.cast::<DeviceChildStateObject<V, T>>();
    // SAFETY: `this` is a valid `DeviceChildStateObject<V, T>`.
    unsafe { (*obj).add_ref() }
}

unsafe extern "system-unwind" fn state_release<V: 'static, T>(this: *mut c_void) -> u32 {
    let obj = this.cast::<DeviceChildStateObject<V, T>>();
    // SAFETY: `this` is a valid `DeviceChildStateObject<V, T>`.
    unsafe { (*obj).release() }
}

unsafe extern "system-unwind" fn state_get_device<V: 'static, T>(
    this: *mut c_void,
    out: *mut *mut c_void,
) {
    if out.is_null() {
        return;
    }
    let obj = this.cast::<DeviceChildStateObject<V, T>>();
    // SAFETY: `this` is a valid `DeviceChildStateObject<V, T>`.
    let dev = unsafe { (*obj).device };
    // SAFETY: `dev` is null or a valid COM device pointer.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = dev };
}

impl<V: 'static, T> HasPrivateData for DeviceChildStateObject<V, T> {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `DeviceChildStateObject<V, T>`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

/// `QueryInterface` of a cached state object.
///
/// Its D3D11 interfaces answer `this`, its D3D10 ones the embedded wrapper; `object` and `name`
/// are the miss line's.
///
/// # Safety
///
/// `this` is a live `DeviceChildStateObject<V, T>`; `riid` and `out` are the application's, null or
/// valid.
unsafe fn state_query_interface<V: 'static, T>(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
    d3d11: &[Guid],
    d3d10: &[Guid],
    object: &Guid,
    name: &str,
) -> i32 {
    let obj = this.cast::<DeviceChildStateObject<V, T>>();
    // SAFETY: `this` is a live state object; only the field's address is taken.
    let d3d10_iface = unsafe { (&raw mut (*obj).d3d10).cast::<c_void>() };
    let take_ref = || {
        // SAFETY: `this` is a live state object.
        unsafe { state_add_ref::<V, T>(this) };
    };
    let qi = Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::Once { object, name },
    };
    // SAFETY: the application's `riid` and `out`, null or valid; `this` is live.
    unsafe {
        qi_table(
            riid,
            out,
            &[(d3d11, this), (d3d10, d3d10_iface)],
            take_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn blend_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `DeviceChildStateObject`; `riid` and `out` are the application's.
    unsafe {
        state_query_interface::<ID3D11BlendState1Vtbl, BlendState>(
            this,
            riid,
            out,
            &[
                IID_IUNKNOWN,
                IID_ID3D11_DEVICE_CHILD,
                IID_ID3D11_BLEND_STATE,
                IID_ID3D11_BLEND_STATE_1,
            ],
            &[
                IID_ID3D10_DEVICE_CHILD,
                IID_ID3D10_BLEND_STATE,
                IID_ID3D10_BLEND_STATE_1,
            ],
            &IID_ID3D11_BLEND_STATE,
            "D3D11BlendState",
        )
    }
}

unsafe extern "system-unwind" fn blend_get_desc(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = BlendState::from_iface(this) {
        // SAFETY: `desc` points to a writable `D3d11BlendDesc`.
        unsafe { *desc.cast::<D3d11BlendDesc>() = demote_blend_desc(&state.desc) };
    }
}

unsafe extern "system-unwind" fn blend_get_desc1(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = BlendState::from_iface(this) {
        // SAFETY: `desc` points to a writable `D3d11BlendDesc1`.
        unsafe { *desc.cast::<D3d11BlendDesc1>() = state.desc };
    }
}

static BLEND_STATE_VTBL: ID3D11BlendState1Vtbl = ID3D11BlendState1Vtbl {
    query_interface: blend_qi,
    add_ref: state_add_ref::<ID3D11BlendState1Vtbl, BlendState>,
    release: state_release::<ID3D11BlendState1Vtbl, BlendState>,
    get_device: state_get_device::<ID3D11BlendState1Vtbl, BlendState>,
    get_private_data: get_private_data::<DeviceChildStateObject<ID3D11BlendState1Vtbl, BlendState>>,
    set_private_data: set_private_data::<DeviceChildStateObject<ID3D11BlendState1Vtbl, BlendState>>,
    set_private_data_interface: set_private_data_interface::<
        DeviceChildStateObject<ID3D11BlendState1Vtbl, BlendState>,
    >,
    get_desc: blend_get_desc,
    get_desc1: blend_get_desc1,
};

unsafe extern "system-unwind" fn rasterizer_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `DeviceChildStateObject`; `riid` and `out` are the application's.
    unsafe {
        state_query_interface::<ID3D11RasterizerState2Vtbl, RasterizerState>(
            this,
            riid,
            out,
            &[
                IID_IUNKNOWN,
                IID_ID3D11_DEVICE_CHILD,
                IID_ID3D11_RASTERIZER_STATE,
                IID_ID3D11_RASTERIZER_STATE_1,
                IID_ID3D11_RASTERIZER_STATE_2,
            ],
            &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_RASTERIZER_STATE],
            &IID_ID3D11_RASTERIZER_STATE,
            "D3D11RasterizerState",
        )
    }
}

unsafe extern "system-unwind" fn rasterizer_get_desc(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = RasterizerState::from_iface(this) {
        let d = &state.desc;
        // SAFETY: `desc` points to a writable `D3d11RasterizerDesc`.
        unsafe {
            *desc.cast::<D3d11RasterizerDesc>() = D3d11RasterizerDesc {
                fill_mode: d.fill_mode,
                cull_mode: d.cull_mode,
                front_counter_clockwise: d.front_counter_clockwise,
                depth_bias: d.depth_bias,
                depth_bias_clamp: d.depth_bias_clamp,
                slope_scaled_depth_bias: d.slope_scaled_depth_bias,
                depth_clip_enable: d.depth_clip_enable,
                scissor_enable: d.scissor_enable,
                multisample_enable: d.multisample_enable,
                antialiased_line_enable: d.antialiased_line_enable,
            };
        }
    }
}

unsafe extern "system-unwind" fn rasterizer_get_desc1(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = RasterizerState::from_iface(this) {
        let d = &state.desc;
        // SAFETY: `desc` points to a writable `D3d11RasterizerDesc1`.
        unsafe {
            *desc.cast::<D3d11RasterizerDesc1>() = D3d11RasterizerDesc1 {
                fill_mode: d.fill_mode,
                cull_mode: d.cull_mode,
                front_counter_clockwise: d.front_counter_clockwise,
                depth_bias: d.depth_bias,
                depth_bias_clamp: d.depth_bias_clamp,
                slope_scaled_depth_bias: d.slope_scaled_depth_bias,
                depth_clip_enable: d.depth_clip_enable,
                scissor_enable: d.scissor_enable,
                multisample_enable: d.multisample_enable,
                antialiased_line_enable: d.antialiased_line_enable,
                forced_sample_count: d.forced_sample_count,
            };
        }
    }
}

unsafe extern "system-unwind" fn rasterizer_get_desc2(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = RasterizerState::from_iface(this) {
        // SAFETY: `desc` points to a writable `D3d11RasterizerDesc2`.
        unsafe { *desc.cast::<D3d11RasterizerDesc2>() = state.desc };
    }
}

static RASTERIZER_STATE_VTBL: ID3D11RasterizerState2Vtbl = ID3D11RasterizerState2Vtbl {
    query_interface: rasterizer_qi,
    add_ref: state_add_ref::<ID3D11RasterizerState2Vtbl, RasterizerState>,
    release: state_release::<ID3D11RasterizerState2Vtbl, RasterizerState>,
    get_device: state_get_device::<ID3D11RasterizerState2Vtbl, RasterizerState>,
    get_private_data: get_private_data::<
        DeviceChildStateObject<ID3D11RasterizerState2Vtbl, RasterizerState>,
    >,
    set_private_data: set_private_data::<
        DeviceChildStateObject<ID3D11RasterizerState2Vtbl, RasterizerState>,
    >,
    set_private_data_interface: set_private_data_interface::<
        DeviceChildStateObject<ID3D11RasterizerState2Vtbl, RasterizerState>,
    >,
    get_desc: rasterizer_get_desc,
    get_desc1: rasterizer_get_desc1,
    get_desc2: rasterizer_get_desc2,
};

unsafe extern "system-unwind" fn depth_stencil_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `DeviceChildStateObject`; `riid` and `out` are the application's.
    unsafe {
        state_query_interface::<ID3D11DepthStencilStateVtbl, DepthStencilState>(
            this,
            riid,
            out,
            &[
                IID_IUNKNOWN,
                IID_ID3D11_DEVICE_CHILD,
                IID_ID3D11_DEPTH_STENCIL_STATE,
            ],
            &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_DEPTH_STENCIL_STATE],
            &IID_ID3D11_DEPTH_STENCIL_STATE,
            "D3D11DepthStencilState",
        )
    }
}

unsafe extern "system-unwind" fn depth_stencil_get_desc(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = DepthStencilState::from_iface(this) {
        // SAFETY: `desc` points to a writable `D3d11DepthStencilDesc`.
        unsafe { *desc.cast::<D3d11DepthStencilDesc>() = state.desc };
    }
}

static DEPTH_STENCIL_STATE_VTBL: ID3D11DepthStencilStateVtbl = ID3D11DepthStencilStateVtbl {
    query_interface: depth_stencil_qi,
    add_ref: state_add_ref::<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    release: state_release::<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    get_device: state_get_device::<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    get_private_data: get_private_data::<
        DeviceChildStateObject<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    >,
    set_private_data: set_private_data::<
        DeviceChildStateObject<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    >,
    set_private_data_interface: set_private_data_interface::<
        DeviceChildStateObject<ID3D11DepthStencilStateVtbl, DepthStencilState>,
    >,
    get_desc: depth_stencil_get_desc,
};

unsafe extern "system-unwind" fn sampler_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `DeviceChildStateObject`; `riid` and `out` are the application's.
    unsafe {
        state_query_interface::<ID3D11SamplerStateVtbl, SamplerState>(
            this,
            riid,
            out,
            &[
                IID_IUNKNOWN,
                IID_ID3D11_DEVICE_CHILD,
                IID_ID3D11_SAMPLER_STATE,
            ],
            &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_SAMPLER_STATE],
            &IID_ID3D11_SAMPLER_STATE,
            "D3D11SamplerState",
        )
    }
}

unsafe extern "system-unwind" fn sampler_get_desc(this: *mut c_void, desc: *mut c_void) {
    if desc.is_null() {
        return;
    }
    if let Some(state) = SamplerState::from_iface(this) {
        // SAFETY: `desc` points to a writable `D3d11SamplerDesc`.
        unsafe { *desc.cast::<D3d11SamplerDesc>() = state.desc };
    }
}

static SAMPLER_STATE_VTBL: ID3D11SamplerStateVtbl = ID3D11SamplerStateVtbl {
    query_interface: sampler_qi,
    add_ref: state_add_ref::<ID3D11SamplerStateVtbl, SamplerState>,
    release: state_release::<ID3D11SamplerStateVtbl, SamplerState>,
    get_device: state_get_device::<ID3D11SamplerStateVtbl, SamplerState>,
    get_private_data: get_private_data::<
        DeviceChildStateObject<ID3D11SamplerStateVtbl, SamplerState>,
    >,
    set_private_data: set_private_data::<
        DeviceChildStateObject<ID3D11SamplerStateVtbl, SamplerState>,
    >,
    set_private_data_interface: set_private_data_interface::<
        DeviceChildStateObject<ID3D11SamplerStateVtbl, SamplerState>,
    >,
    get_desc: sampler_get_desc,
};

/// Default D3D11 `D3D11_BLEND_DESC1` (`kDefaultBlendDesc`, `d3d11_state_object.hpp:198-201`).
#[must_use]
pub const fn default_blend_desc1() -> D3d11BlendDesc1 {
    let rt0 = D3d11RenderTargetBlendDesc1 {
        blend_enable: 0,
        logic_op_enable: 0,
        src_blend: 2,
        dest_blend: 1,
        blend_op: 1,
        src_blend_alpha: 2,
        dest_blend_alpha: 1,
        blend_op_alpha: 1,
        logic_op: 4,
        render_target_write_mask: 0x0f,
    };
    let zero_rt = D3d11RenderTargetBlendDesc1 {
        blend_enable: 0,
        logic_op_enable: 0,
        src_blend: 0,
        dest_blend: 0,
        blend_op: 0,
        src_blend_alpha: 0,
        dest_blend_alpha: 0,
        blend_op_alpha: 0,
        logic_op: 0,
        render_target_write_mask: 0,
    };
    D3d11BlendDesc1 {
        alpha_to_coverage_enable: 0,
        independent_blend_enable: 0,
        render_target: [
            rt0, zero_rt, zero_rt, zero_rt, zero_rt, zero_rt, zero_rt, zero_rt,
        ],
    }
}

/// Default D3D11 `D3D11_RASTERIZER_DESC2`.
#[must_use]
pub const fn default_rasterizer_desc2() -> D3d11RasterizerDesc2 {
    D3d11RasterizerDesc2 {
        fill_mode: 3,
        cull_mode: 3,
        front_counter_clockwise: 0,
        depth_bias: 0,
        depth_bias_clamp: 0.0,
        slope_scaled_depth_bias: 0.0,
        depth_clip_enable: 1,
        scissor_enable: 0,
        multisample_enable: 0,
        antialiased_line_enable: 0,
        forced_sample_count: 0,
        conservative_raster: 0,
    }
}

/// Default D3D11 `D3D11_DEPTH_STENCIL_DESC`.
#[must_use]
pub const fn default_depth_stencil_desc() -> D3d11DepthStencilDesc {
    let face = D3d11DepthStencilopDesc {
        stencil_fail_op: 1,
        stencil_depth_fail_op: 1,
        stencil_pass_op: 1,
        stencil_func: 8,
    };
    D3d11DepthStencilDesc {
        depth_enable: 1,
        depth_write_mask: 1,
        depth_func: 2,
        stencil_enable: 0,
        stencil_read_mask: 0xff,
        stencil_write_mask: 0xff,
        front_face: face,
        back_face: face,
    }
}

/// Default D3D11 `D3D11_SAMPLER_DESC` (`kDefaultSamplerDesc`, `d3d11_state_object.hpp:203-214`).
#[must_use]
pub const fn default_sampler_desc() -> D3d11SamplerDesc {
    D3d11SamplerDesc {
        filter: 0x15,
        address_u: 3,
        address_v: 3,
        address_w: 3,
        mip_lod_bias: 0.0,
        max_anisotropy: 1,
        comparison_func: 1,
        border_color: [0.0, 0.0, 0.0, 0.0],
        min_lod: 0.0,
        max_lod: 0.0,
    }
}

/// Default state objects created per device (`CreateDefault*State`).
pub struct Defaults {
    pub blend: *mut c_void,
    pub rasterizer: *mut c_void,
    pub depth_stencil: *mut c_void,
    pub sampler: *mut c_void,
}

impl Default for Defaults {
    fn default() -> Self {
        Self::new(ptr::null_mut())
    }
}

impl Defaults {
    /// Creates default state objects for `device`.
    #[must_use]
    pub fn new(device: *mut c_void) -> Self {
        let (caches, metal_device) = caches_for_device(device);

        let blend_desc = default_blend_desc1();
        let blend = caches
            .blend
            .get_or_create(CanonicalBlendKey::from_desc(&blend_desc), || {
                Ok(DeviceChildStateObject::new_with_d3d10(
                    &BLEND_STATE_VTBL,
                    &crate::d3d10::BLEND_STATE_VTBL,
                    device,
                    translate_blend_state(&blend_desc)?,
                ))
            })
            .map_or(ptr::null_mut(), |ptr| {
                // SAFETY: `get_or_create` returned a +1 reference; `Defaults` holds a non-owning pointer.
                unsafe { (*ptr).release() };
                ptr.cast()
            });

        let rast_desc = default_rasterizer_desc2();
        let rasterizer = caches
            .rasterizer
            .get_or_create(CanonicalRasterizerKey::from_desc(&rast_desc), || {
                Ok(DeviceChildStateObject::new_with_d3d10(
                    &RASTERIZER_STATE_VTBL,
                    &crate::d3d10::RASTERIZER_STATE_VTBL,
                    device,
                    translate_rasterizer_state(&rast_desc)?,
                ))
            })
            .map_or(ptr::null_mut(), |ptr| {
                // SAFETY: `get_or_create` returned a +1 reference; `Defaults` holds a non-owning pointer.
                unsafe { (*ptr).release() };
                ptr.cast()
            });

        let ds_desc = default_depth_stencil_desc();
        let depth_stencil = caches
            .depth_stencil
            .get_or_create(CanonicalDepthStencilKey::from_desc(&ds_desc), || {
                Ok(DeviceChildStateObject::new_with_d3d10(
                    &DEPTH_STENCIL_STATE_VTBL,
                    &crate::d3d10::DEPTH_STENCIL_STATE_VTBL,
                    device,
                    translate_depth_stencil_state(
                        &ds_desc,
                        metal_device.map(|dev| (dev, &caches.metal_depth_stencil)),
                    )?,
                ))
            })
            .map_or(ptr::null_mut(), |ptr| {
                // SAFETY: `get_or_create` returned a +1 reference; `Defaults` holds a non-owning pointer.
                unsafe { (*ptr).release() };
                ptr.cast()
            });

        let samp_desc = default_sampler_desc();
        let sampler = caches
            .sampler
            .get_or_create(CanonicalSamplerKey::from_desc(&samp_desc), || {
                Ok(DeviceChildStateObject::new_with_d3d10(
                    &SAMPLER_STATE_VTBL,
                    &crate::d3d10::SAMPLER_STATE_VTBL,
                    device,
                    translate_sampler_state(&samp_desc, metal_device)?,
                ))
            })
            .map_or(ptr::null_mut(), |ptr| {
                // SAFETY: `get_or_create` returned a +1 reference; `Defaults` holds a non-owning pointer.
                unsafe { (*ptr).release() };
                ptr.cast()
            });

        Self {
            blend,
            rasterizer,
            depth_stencil,
            sampler,
        }
    }
}

fn create_blend_state_impl(
    device: *mut c_void,
    desc: *const D3d11BlendDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let desc1 = promote_blend_desc(unsafe { &*desc });
    create_blend_state1_impl(device, &raw const desc1, out)
}

/// `ID3D11Device::CreateBlendState` (`d3d11_device.cpp`).
pub fn create_blend_state(
    device: *mut c_void,
    desc: *const D3d11BlendDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_blend_state_impl(device, desc, out)
}

fn create_blend_state1_impl(
    device: *mut c_void,
    desc: *const D3d11BlendDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d = unsafe { &*desc };
    let translated = match translate_blend_state(d) {
        Ok(s) => s,
        Err(hr) => return hr,
    };
    if out.is_null() {
        return S_FALSE;
    }
    let (caches, _) = caches_for_device(device);
    let key = CanonicalBlendKey::from_desc(d);
    match caches.blend.get_or_create(key, || {
        Ok(DeviceChildStateObject::new_with_d3d10(
            &BLEND_STATE_VTBL,
            &crate::d3d10::BLEND_STATE_VTBL,
            device,
            translated,
        ))
    }) {
        Ok(ptr) => {
            // SAFETY: `out` is non-null.
            unsafe { *out = ptr.cast() };
            S_OK
        }
        Err(hr) => hr,
    }
}

/// `ID3D11Device1::CreateBlendState1` (`d3d11_device.cpp`).
pub fn create_blend_state1(
    device: *mut c_void,
    desc: *const D3d11BlendDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_blend_state1_impl(device, desc, out)
}

fn create_rasterizer_state_impl(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let desc2 = promote_rasterizer_desc(unsafe { &*desc });
    create_rasterizer_state2_impl(device, &raw const desc2, out)
}

/// `ID3D11Device::CreateRasterizerState` (`d3d11_device.cpp`).
pub fn create_rasterizer_state(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_rasterizer_state_impl(device, desc, out)
}

fn create_rasterizer_state1_impl(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let desc2 = promote_rasterizer_desc1(unsafe { &*desc });
    create_rasterizer_state2_impl(device, &raw const desc2, out)
}

/// `ID3D11Device1::CreateRasterizerState1` (`d3d11_device.cpp`).
pub fn create_rasterizer_state1(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_rasterizer_state1_impl(device, desc, out)
}

fn create_rasterizer_state2_impl(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc2,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d = unsafe { &*desc };
    let translated = match translate_rasterizer_state(d) {
        Ok(s) => s,
        Err(hr) => return hr,
    };
    if out.is_null() {
        return S_FALSE;
    }
    let (caches, _) = caches_for_device(device);
    let key = CanonicalRasterizerKey::from_desc(d);
    match caches.rasterizer.get_or_create(key, || {
        Ok(DeviceChildStateObject::new_with_d3d10(
            &RASTERIZER_STATE_VTBL,
            &crate::d3d10::RASTERIZER_STATE_VTBL,
            device,
            translated,
        ))
    }) {
        Ok(ptr) => {
            // SAFETY: `out` is non-null.
            unsafe { *out = ptr.cast() };
            S_OK
        }
        Err(hr) => hr,
    }
}

/// `ID3D11Device3::CreateRasterizerState2` (`d3d11_device.cpp`).
pub fn create_rasterizer_state2(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc2,
    out: *mut *mut c_void,
) -> Hresult {
    create_rasterizer_state2_impl(device, desc, out)
}

fn create_depth_stencil_state_impl(
    device: *mut c_void,
    desc: *const D3d11DepthStencilDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d = unsafe { &*desc };
    let (caches, metal_device) = caches_for_device(device);
    if out.is_null() {
        return S_FALSE;
    }
    let key = CanonicalDepthStencilKey::from_desc(d);
    match caches.depth_stencil.get_or_create(key, || {
        let metal = metal_device.map(|dev| (dev, &caches.metal_depth_stencil));
        let translated = translate_depth_stencil_state(d, metal)?;
        Ok(DeviceChildStateObject::new_with_d3d10(
            &DEPTH_STENCIL_STATE_VTBL,
            &crate::d3d10::DEPTH_STENCIL_STATE_VTBL,
            device,
            translated,
        ))
    }) {
        Ok(ptr) => {
            // SAFETY: `out` is non-null.
            unsafe { *out = ptr.cast() };
            S_OK
        }
        Err(hr) => hr,
    }
}

/// `ID3D11Device::CreateDepthStencilState` (`d3d11_device.cpp`).
pub fn create_depth_stencil_state(
    device: *mut c_void,
    desc: *const D3d11DepthStencilDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_depth_stencil_state_impl(device, desc, out)
}

fn create_sampler_state_impl(
    device: *mut c_void,
    desc: *const D3d11SamplerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d = unsafe { &*desc };
    if out.is_null() {
        return S_FALSE;
    }
    let (caches, metal_device) = caches_for_device(device);
    let key = CanonicalSamplerKey::from_desc(d);
    match caches.sampler.get_or_create(key, || {
        let translated = translate_sampler_state(d, metal_device)?;
        Ok(DeviceChildStateObject::new_with_d3d10(
            &SAMPLER_STATE_VTBL,
            &crate::d3d10::SAMPLER_STATE_VTBL,
            device,
            translated,
        ))
    }) {
        Ok(ptr) => {
            // SAFETY: `out` is non-null.
            unsafe { *out = ptr.cast() };
            S_OK
        }
        Err(hr) => hr,
    }
}

/// `ID3D11Device::CreateSamplerState` (`d3d11_device.cpp`).
pub fn create_sampler_state(
    device: *mut c_void,
    desc: *const D3d11SamplerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_sampler_state_impl(device, desc, out)
}

#[cfg(test)]
mod tests;
