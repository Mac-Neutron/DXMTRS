//! Deduplicating caches for blend, depth-stencil, rasterizer and sampler states.

use core::{
    ffi::c_void,
    hash::Hash,
    num::NonZeroU64,
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
use std::sync::Mutex;

use dxmt_util::private_data::PrivateData;
use rustc_hash::FxHashMap;

use crate::{
    abi::{
        records::{D3d11BlendDesc1, D3d11DepthStencilDesc, D3d11RasterizerDesc2, D3d11SamplerDesc},
        vtbl::{
            ID3D11BlendState1Vtbl, ID3D11DepthStencilStateVtbl, ID3D11RasterizerState2Vtbl,
            ID3D11SamplerStateVtbl,
        },
    },
    com::{Hresult, add_ref, release},
    state::{BlendState, DepthStencilState, RasterizerState, SamplerState},
};

/// Cached COM state object (`MTLD3D11StateObject`, `d3d11_device_child.hpp`).
///
/// The public reference count starts at `0` while the object resides in a [`StateObjectCache`].
/// Transitioning from `0` to `1` takes one reference on `device`; transitioning from `1` to `0`
/// releases that reference without destroying `self` (which lives until the owning cache drops).
#[repr(C)]
pub struct DeviceChildStateObject<V: 'static, T> {
    pub vtbl: &'static V,
    pub refcount: AtomicU32,
    pub device: *mut c_void,
    pub private_data: PrivateData,
    pub inner: T,
    pub d3d10: crate::d3d10::Wrapper<u8>,
}

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "d3d10::Wrapper holds an immutable pointer back to the enclosing state object"
)]
// SAFETY: cached state objects are immutable after construction; `refcount` is atomic and
// `private_data` is internally synchronized.
unsafe impl<V: 'static + Sync, T: Send + Sync> Send for DeviceChildStateObject<V, T> {}
// SAFETY: see above.
unsafe impl<V: 'static + Sync, T: Send + Sync> Sync for DeviceChildStateObject<V, T> {}

static DUMMY_D3D10_VTBL: u8 = 0;

impl<V: 'static, T> DeviceChildStateObject<V, T> {
    /// Allocates a new cached state object with `refcount == 0`.
    #[must_use]
    pub fn new(vtbl: &'static V, device: *mut c_void, inner: T) -> Box<Self> {
        Self::new_with_d3d10(vtbl, &DUMMY_D3D10_VTBL, device, inner)
    }

    /// Allocates a new cached state object with `refcount == 0` and an embedded D3D10 wrapper.
    #[must_use]
    pub fn new_with_d3d10<D: 'static>(
        vtbl: &'static V,
        d3d10_vtbl: &'static D,
        device: *mut c_void,
        inner: T,
    ) -> Box<Self> {
        let imm_ctx = crate::device::Device::from_iface(device)
            .map_or(ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        // SAFETY: `d3d10_vtbl` is a `'static` vtable reference; `Wrapper<u8>` stores its address.
        let vtbl_u8 = unsafe { &*ptr::from_ref(d3d10_vtbl).cast::<u8>() };
        let mut boxed = Box::new(Self {
            vtbl,
            refcount: AtomicU32::new(0),
            device,
            private_data: PrivateData::default(),
            inner,
            d3d10: crate::d3d10::Wrapper::new(vtbl_u8, ptr::null_mut(), imm_ctx),
        });
        let self_ptr = ptr::from_mut::<Self>(&mut *boxed).cast::<c_void>();
        boxed.d3d10.d3d11 = self_ptr;
        boxed
    }

    /// Raw COM interface pointer for this state object.
    #[must_use]
    pub const fn as_iface(&self) -> *mut c_void {
        ptr::from_ref(self).cast_mut().cast()
    }

    /// Increments the external reference count, adding a reference to `device` on `0 -> 1`.
    pub fn add_ref(&self) -> u32 {
        let prev = self.refcount.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            // SAFETY: `self.device` is either null or a live COM device pointer.
            unsafe { add_ref(self.device) };
        }
        prev + 1
    }

    /// Decrements the external reference count, releasing `device` on `1 -> 0`.
    pub fn release(&self) -> u32 {
        let prev = self.refcount.fetch_sub(1, Ordering::SeqCst);
        let remaining = prev.saturating_sub(1);
        if prev == 1 {
            // SAFETY: `self.device` is either null or a live COM device pointer.
            unsafe { release(self.device) };
        }
        remaining
    }

    /// Current external reference count.
    #[must_use]
    pub fn ref_count(&self) -> u32 {
        self.refcount.load(Ordering::SeqCst)
    }
}

/// Thread-safe deduplicating cache of [`DeviceChildStateObject`] instances keyed by `K`.
pub struct StateObjectCache<K, V: 'static, T> {
    entries: Mutex<FxHashMap<K, Box<DeviceChildStateObject<V, T>>>>,
}

impl<K, V: 'static, T> Default for StateObjectCache<K, V, T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(FxHashMap::default()),
        }
    }
}

impl<K: Eq + Hash, V: 'static, T> StateObjectCache<K, V, T> {
    /// Looks up `key` or inserts the state object produced by `create`, incrementing its refcount.
    ///
    /// # Errors
    ///
    /// Propagates any `Hresult` error returned by `create`.
    pub fn get_or_create<F>(
        &self,
        key: K,
        create: F,
    ) -> Result<*mut DeviceChildStateObject<V, T>, Hresult>
    where
        F: FnOnce() -> Result<Box<DeviceChildStateObject<V, T>>, Hresult>,
    {
        let mut map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&key) {
            existing.add_ref();
            return Ok(ptr::from_ref::<DeviceChildStateObject<V, T>>(&**existing).cast_mut());
        }
        let created = create()?;
        created.add_ref();
        let raw = ptr::from_ref::<DeviceChildStateObject<V, T>>(&*created).cast_mut();
        map.insert(key, created);
        drop(map);
        Ok(raw)
    }

    /// Number of distinct state objects currently stored in this cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Returns `true` when the cache holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

static NEXT_NAN_NONCE: AtomicU64 = AtomicU64::new(1);

fn next_nan_nonce() -> Option<NonZeroU64> {
    NonZeroU64::new(NEXT_NAN_NONCE.fetch_add(1, Ordering::Relaxed))
}

const fn canon_f32(v: f32) -> u32 {
    if v == 0.0 { 0 } else { v.to_bits() }
}

/// Canonicalized single render-target blend descriptor key.
#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct CanonicalRtBlendKey {
    pub blend_enable: bool,
    pub logic_op_enable: bool,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub logic_op: u32,
    pub render_target_write_mask: u8,
}

/// Canonicalized `D3D11_BLEND_DESC1` key for [`StateObjectCache`].
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CanonicalBlendKey {
    pub alpha_to_coverage_enable: bool,
    pub independent_blend_enable: bool,
    pub render_target: [CanonicalRtBlendKey; 8],
}

impl CanonicalBlendKey {
    /// Canonicalizes `desc`, normalizing `BOOL`s and zeroing inactive render-target slots.
    #[must_use]
    pub fn from_desc(desc: &D3d11BlendDesc1) -> Self {
        let alpha_to_coverage_enable = desc.alpha_to_coverage_enable != 0;
        let independent_blend_enable = desc.independent_blend_enable != 0;
        let mut render_target: [CanonicalRtBlendKey; 8] = Default::default();
        let count = if independent_blend_enable { 8 } else { 1 };
        for (dst, s) in render_target
            .iter_mut()
            .zip(&desc.render_target)
            .take(count)
        {
            let blend_enable = s.blend_enable != 0;
            let logic_op_enable = s.logic_op_enable != 0;
            *dst = CanonicalRtBlendKey {
                blend_enable,
                logic_op_enable,
                src_blend: if blend_enable { s.src_blend } else { 0 },
                dest_blend: if blend_enable { s.dest_blend } else { 0 },
                blend_op: if blend_enable { s.blend_op } else { 0 },
                src_blend_alpha: if blend_enable { s.src_blend_alpha } else { 0 },
                dest_blend_alpha: if blend_enable { s.dest_blend_alpha } else { 0 },
                blend_op_alpha: if blend_enable { s.blend_op_alpha } else { 0 },
                logic_op: if logic_op_enable { s.logic_op } else { 0 },
                render_target_write_mask: s.render_target_write_mask & 0x0f,
            };
        }
        Self {
            alpha_to_coverage_enable,
            independent_blend_enable,
            render_target,
        }
    }
}

/// Canonicalized `D3D11_RASTERIZER_DESC2` key for [`StateObjectCache`].
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CanonicalRasterizerKey {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub depth_bias: i32,
    pub depth_bias_clamp_bits: u32,
    pub slope_scaled_depth_bias_bits: u32,
    pub forced_sample_count: u32,
    pub conservative_raster: u32,
    pub bool_flags: u8,
    nan_nonce: Option<NonZeroU64>,
}

impl CanonicalRasterizerKey {
    /// Canonicalizes `desc`, normalizing `BOOL`s and float bit representations.
    ///
    /// A descriptor containing `NaN` in any floating-point field receives a unique nonce so it
    /// never compares equal to another descriptor (`nan_desc_never_equals`).
    #[must_use]
    pub fn from_desc(desc: &D3d11RasterizerDesc2) -> Self {
        let has_nan = desc.depth_bias_clamp.is_nan() || desc.slope_scaled_depth_bias.is_nan();
        let bool_flags = u8::from(desc.front_counter_clockwise != 0)
            | (u8::from(desc.depth_clip_enable != 0) << 1)
            | (u8::from(desc.scissor_enable != 0) << 2)
            | (u8::from(desc.multisample_enable != 0) << 3)
            | (u8::from(desc.antialiased_line_enable != 0) << 4);
        Self {
            fill_mode: desc.fill_mode,
            cull_mode: desc.cull_mode,
            depth_bias: desc.depth_bias,
            depth_bias_clamp_bits: canon_f32(desc.depth_bias_clamp),
            slope_scaled_depth_bias_bits: canon_f32(desc.slope_scaled_depth_bias),
            forced_sample_count: desc.forced_sample_count,
            conservative_raster: desc.conservative_raster,
            bool_flags,
            nan_nonce: if has_nan { next_nan_nonce() } else { None },
        }
    }
}

/// Canonicalized stencil-face operation key.
#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct CanonicalStencilOpKey {
    pub stencil_fail_op: u32,
    pub stencil_depth_fail_op: u32,
    pub stencil_pass_op: u32,
    pub stencil_func: u32,
}

impl CanonicalStencilOpKey {
    const fn zero() -> Self {
        Self {
            stencil_fail_op: 0,
            stencil_depth_fail_op: 0,
            stencil_pass_op: 0,
            stencil_func: 0,
        }
    }
}

/// Canonicalized `D3D11_DEPTH_STENCIL_DESC` key for [`StateObjectCache`].
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CanonicalDepthStencilKey {
    pub depth_enable: bool,
    pub depth_write_mask: bool,
    pub depth_func: u32,
    pub stencil_enable: bool,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: CanonicalStencilOpKey,
    pub back_face: CanonicalStencilOpKey,
}

impl CanonicalDepthStencilKey {
    /// Canonicalizes `desc`, normalizing `BOOL`s and zeroing disabled depth or stencil fields.
    #[must_use]
    pub const fn from_desc(desc: &D3d11DepthStencilDesc) -> Self {
        let depth_enable = desc.depth_enable != 0;
        let stencil_enable = desc.stencil_enable != 0;
        Self {
            depth_enable,
            depth_write_mask: depth_enable && (desc.depth_write_mask != 0),
            depth_func: if depth_enable { desc.depth_func } else { 0 },
            stencil_enable,
            stencil_read_mask: if stencil_enable {
                desc.stencil_read_mask
            } else {
                0
            },
            stencil_write_mask: if stencil_enable {
                desc.stencil_write_mask
            } else {
                0
            },
            front_face: if stencil_enable {
                CanonicalStencilOpKey {
                    stencil_fail_op: desc.front_face.stencil_fail_op,
                    stencil_depth_fail_op: desc.front_face.stencil_depth_fail_op,
                    stencil_pass_op: desc.front_face.stencil_pass_op,
                    stencil_func: desc.front_face.stencil_func,
                }
            } else {
                CanonicalStencilOpKey::zero()
            },
            back_face: if stencil_enable {
                CanonicalStencilOpKey {
                    stencil_fail_op: desc.back_face.stencil_fail_op,
                    stencil_depth_fail_op: desc.back_face.stencil_depth_fail_op,
                    stencil_pass_op: desc.back_face.stencil_pass_op,
                    stencil_func: desc.back_face.stencil_func,
                }
            } else {
                CanonicalStencilOpKey::zero()
            },
        }
    }
}

/// Canonicalized `D3D11_SAMPLER_DESC` key for [`StateObjectCache`].
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CanonicalSamplerKey {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias_bits: u32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color_bits: [u32; 4],
    pub min_lod_bits: u32,
    pub max_lod_bits: u32,
    nan_nonce: Option<NonZeroU64>,
}

impl CanonicalSamplerKey {
    /// Canonicalizes `desc`, normalizing float bit representations (`-0.0` to `0.0`).
    ///
    /// A descriptor containing `NaN` in any floating-point field receives a unique nonce so it
    /// never compares equal to another descriptor (`nan_desc_never_equals`).
    #[must_use]
    pub fn from_desc(desc: &D3d11SamplerDesc) -> Self {
        let has_nan = desc.mip_lod_bias.is_nan()
            || desc.border_color[0].is_nan()
            || desc.border_color[1].is_nan()
            || desc.border_color[2].is_nan()
            || desc.border_color[3].is_nan()
            || desc.min_lod.is_nan()
            || desc.max_lod.is_nan();
        Self {
            filter: desc.filter,
            address_u: desc.address_u,
            address_v: desc.address_v,
            address_w: desc.address_w,
            mip_lod_bias_bits: canon_f32(desc.mip_lod_bias),
            max_anisotropy: desc.max_anisotropy,
            comparison_func: desc.comparison_func,
            border_color_bits: [
                canon_f32(desc.border_color[0]),
                canon_f32(desc.border_color[1]),
                canon_f32(desc.border_color[2]),
                canon_f32(desc.border_color[3]),
            ],
            min_lod_bits: canon_f32(desc.min_lod),
            max_lod_bits: canon_f32(desc.max_lod),
            nan_nonce: if has_nan { next_nan_nonce() } else { None },
        }
    }
}

/// Per-device caches of blend, rasterizer, depth-stencil and sampler state objects.
#[derive(Default)]
pub struct StateCaches {
    pub blend: StateObjectCache<CanonicalBlendKey, ID3D11BlendState1Vtbl, BlendState>,
    pub rasterizer:
        StateObjectCache<CanonicalRasterizerKey, ID3D11RasterizerState2Vtbl, RasterizerState>,
    pub depth_stencil:
        StateObjectCache<CanonicalDepthStencilKey, ID3D11DepthStencilStateVtbl, DepthStencilState>,
    pub sampler: StateObjectCache<CanonicalSamplerKey, ID3D11SamplerStateVtbl, SamplerState>,
}
