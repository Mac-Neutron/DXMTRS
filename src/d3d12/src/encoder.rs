//! D3D12 command encoder records (`d3d12_command_encoder.hpp`) and submission contract.
//!
//! `CommandList` (`list`) and `CommandAllocator` (`allocator`) record passes as linked lists of
//! [`EncoderData`]-headed nodes in the allocator's arena, and `CommandQueue` (`queue`) orders,
//! merges, folds, and encodes them into Metal command encoders.

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{
    resource::{Texture, TextureAllocation, TextureViewKey},
    scaler::Scaler,
};
use winemetal_wire::{
    WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_STORE, WMTClearColor,
    WMTFXTemporalScalerInfo, WMTFXTemporalScalerProps, WmtcmdBase, WmtcmdBlitNop, WmtcmdComputeNop,
    WmtcmdRenderNop,
};

/// A `MetalFX` temporal scaler paired with its [`WMTFXTemporalScalerInfo`] (`dxmt_scaler.hpp`).
pub struct TemporalScaler {
    pub scaler: Scaler,
    pub info: WMTFXTemporalScalerInfo,
}

impl TemporalScaler {
    /// Creates a temporal scaler and its fence on `device` for `info`.
    ///
    /// # Safety
    ///
    /// `device` must be a live `MTLDevice` handle.
    #[must_use]
    pub unsafe fn new(device: u64, info: WMTFXTemporalScalerInfo) -> Self {
        // SAFETY: `device` is a live `MTLDevice` and `info` is valid for the call.
        let scaler = unsafe { Scaler::temporal(device, &info) };
        Self { scaler, info }
    }

    /// The `MTLFXTemporalScaler` handle, or `None` when Metal made none.
    #[must_use]
    pub const fn scaler(&self) -> Option<&winemetal_sys::Obj> {
        self.scaler.scaler()
    }

    /// The scaler's `MTLFence` handle, or `None` when Metal made none.
    #[must_use]
    pub const fn fence(&self) -> Option<&winemetal_sys::Obj> {
        self.scaler.fence()
    }

    /// The description this scaler was created for.
    #[must_use]
    pub const fn info(&self) -> &WMTFXTemporalScalerInfo {
        &self.info
    }
}

/// Strong reference to a [`Texture`], its [`TextureAllocation`], and a cached view key.
///
/// Equality compares `(Arc::as_ptr(&allocation), key)`, matching C++ `TextureView *` pointer
/// identity (`allocation->cached_view_[key]`).
pub struct TextureViewRef {
    pub descriptor: Arc<Texture>,
    pub allocation: Arc<TextureAllocation>,
    pub key: TextureViewKey,
    pub texture: u64,
    pub gpu_resource_id: u64,
}

impl Clone for TextureViewRef {
    fn clone(&self) -> Self {
        Self {
            descriptor: Arc::clone(&self.descriptor),
            allocation: Arc::clone(&self.allocation),
            key: self.key,
            texture: self.texture,
            gpu_resource_id: self.gpu_resource_id,
        }
    }
}

impl TextureViewRef {
    /// Resolves `key` on `descriptor`'s current allocation.
    #[must_use]
    pub fn from_texture(descriptor: &Arc<Texture>, key: TextureViewKey) -> Option<Self> {
        let allocation = descriptor.current()?;
        let view = allocation.view(key);
        Some(Self {
            descriptor: Arc::clone(descriptor),
            allocation,
            key,
            texture: view.texture,
            gpu_resource_id: view.gpu_resource_id,
        })
    }

    /// The view's `MTLTexture` handle.
    #[must_use]
    pub const fn texture(&self) -> u64 {
        self.texture
    }

    /// Raw pointer to the parent [`Texture`] descriptor for [`EncoderData::writes`].
    #[must_use]
    pub fn descriptor_ptr(&self) -> *const c_void {
        Arc::as_ptr(&self.descriptor).cast::<c_void>()
    }
}

impl PartialEq for TextureViewRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation) && self.key == other.key
    }
}

impl Eq for TextureViewRef {}

/// Pass encoder kind (`dxmt::d3d12::EncoderType` in `d3d12_command_encoder.hpp:29`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EncoderType {
    Null = 0,
    Clear = 1,
    Render = 2,
    Blit = 3,
    Compute = 4,
    Resolve = 5,
    TemporalUpscale = 6,
}

/// Timestamp sample recorded at the end of an encoder (`d3d12_command_encoder.hpp:44`).
#[repr(C)]
pub struct EncoderSample {
    /// Query heap's Metal counter sample buffer handle (`0` when CPU-fallback only).
    pub buffer: u64,
    /// Query index inside the counter sample buffer.
    pub index: u32,
    /// CPU-side query slot written by the queue when `buffer == 0` (or alongside it).
    pub value: *mut u64,
}

impl EncoderSample {
    /// An empty sample slot.
    pub const EMPTY: Self = Self {
        buffer: 0,
        index: 0,
        value: ptr::null_mut(),
    };
}

impl Clone for EncoderSample {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer,
            index: self.index,
            value: self.value,
        }
    }
}

impl Default for EncoderSample {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Common header at offset 0 of every encoder node (`d3d12_command_encoder.hpp:39`).
#[repr(C)]
pub struct EncoderData {
    pub kind: EncoderType,
    pub next: *mut Self,
    pub id: u64,
    pub samples: [EncoderSample; 4],
    pub num_samples: u8,
    pub barriers: u32,
    pub barriers_last: u32,
    pub position: u32,
    pub join: bool,
    pub writes_unknown: bool,
    pub write_count: u8,
    pub writes: [*const c_void; 16],
    pub dep_count: u32,
    pub deps: *const u32,
    pub timestamp_only: bool,
    pub query_resolve_only: bool,
}

impl EncoderData {
    /// Initializes an `EncoderData` header of `kind` with `id`.
    #[must_use]
    pub const fn new(kind: EncoderType, id: u64) -> Self {
        Self {
            kind,
            next: ptr::null_mut(),
            id,
            samples: [EncoderSample::EMPTY; 4],
            num_samples: 0,
            barriers: 0,
            barriers_last: 0,
            position: 0,
            join: false,
            writes_unknown: true,
            write_count: 0,
            writes: [ptr::null(); 16],
            dep_count: 0,
            deps: ptr::null(),
            timestamp_only: false,
            query_resolve_only: false,
        }
    }
}

/// Clear-only pass (`ClearEncoderData`, `d3d12_command_encoder.hpp:67`).
#[repr(C)]
pub struct ClearEncoderData {
    pub base: EncoderData,
    pub color: WMTClearColor,
    pub depth_stencil: (f32, u8),
    pub attachment: Option<TextureViewRef>,
    pub clear_dsv: u32,
    pub array_length: u32,
    pub width: u32,
    pub height: u32,
    pub depth_plane: u32,
}

/// Color attachment state on a [`RenderEncoderData`] (`d3d12_command_encoder.hpp:82`).
pub struct RenderEncoderColorAttachmentData {
    pub attachment: Option<TextureViewRef>,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_color: WMTClearColor,
    pub resolve_attachment: Option<TextureViewRef>,
    pub resolve_level: u16,
    pub resolve_slice: u16,
    pub resolve_depth_plane: u32,
}

impl Clone for RenderEncoderColorAttachmentData {
    fn clone(&self) -> Self {
        Self {
            attachment: self.attachment.clone(),
            load_action: self.load_action,
            store_action: self.store_action,
            level: self.level,
            slice: self.slice,
            depth_plane: self.depth_plane,
            clear_color: self.clear_color,
            resolve_attachment: self.resolve_attachment.clone(),
            resolve_level: self.resolve_level,
            resolve_slice: self.resolve_slice,
            resolve_depth_plane: self.resolve_depth_plane,
        }
    }
}

impl Default for RenderEncoderColorAttachmentData {
    fn default() -> Self {
        Self {
            attachment: None,
            load_action: WMT_LOAD_ACTION_LOAD,
            store_action: WMT_STORE_ACTION_STORE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_color: WMTClearColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            resolve_attachment: None,
            resolve_level: 0,
            resolve_slice: 0,
            resolve_depth_plane: 0,
        }
    }
}

/// Depth attachment state on a [`RenderEncoderData`] (`d3d12_command_encoder.hpp:96`).
pub struct RenderEncoderDepthAttachmentData {
    pub attachment: Option<TextureViewRef>,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_depth: f32,
}

impl Clone for RenderEncoderDepthAttachmentData {
    fn clone(&self) -> Self {
        Self {
            attachment: self.attachment.clone(),
            load_action: self.load_action,
            store_action: self.store_action,
            level: self.level,
            slice: self.slice,
            depth_plane: self.depth_plane,
            clear_depth: self.clear_depth,
        }
    }
}

impl Default for RenderEncoderDepthAttachmentData {
    fn default() -> Self {
        Self {
            attachment: None,
            load_action: WMT_LOAD_ACTION_LOAD,
            store_action: WMT_STORE_ACTION_DONT_CARE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_depth: 1.0,
        }
    }
}

/// Stencil attachment state on a [`RenderEncoderData`] (`d3d12_command_encoder.hpp:106`).
pub struct RenderEncoderStencilAttachmentData {
    pub attachment: Option<TextureViewRef>,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_stencil: u8,
}

impl Clone for RenderEncoderStencilAttachmentData {
    fn clone(&self) -> Self {
        Self {
            attachment: self.attachment.clone(),
            load_action: self.load_action,
            store_action: self.store_action,
            level: self.level,
            slice: self.slice,
            depth_plane: self.depth_plane,
            clear_stencil: self.clear_stencil,
        }
    }
}

impl Default for RenderEncoderStencilAttachmentData {
    fn default() -> Self {
        Self {
            attachment: None,
            load_action: WMT_LOAD_ACTION_LOAD,
            store_action: WMT_STORE_ACTION_DONT_CARE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_stencil: 0,
        }
    }
}

/// Render pass encoder record (`RenderEncoderData`, `d3d12_command_encoder.hpp:116`).
#[repr(C)]
pub struct RenderEncoderData {
    pub base: EncoderData,
    pub colors: [RenderEncoderColorAttachmentData; 8],
    pub depth: RenderEncoderDepthAttachmentData,
    pub stencil: RenderEncoderStencilAttachmentData,
    pub default_raster_sample_count: u8,
    pub render_target_array_length: u16,
    pub render_target_height: u32,
    pub render_target_width: u32,
    pub cmd_head: WmtcmdRenderNop,
    pub cmd_tail: *mut WmtcmdBase,
    pub dsv_planar_flags: u8,
    pub dsv_readonly_flags: u8,
    pub render_target_count: u8,
    pub use_visibility_result: bool,
    pub visibility_buffer: u64,
    pub use_tessellation: bool,
    pub use_geometry: bool,
    pub uav_writes: bool,
    pub pre_head: WmtcmdComputeNop,
    pub pre_tail: *mut WmtcmdBase,
}

/// Return value of [`clear_slot`] when `c` clears the depth/stencil attachment of `r`.
pub const CLEAR_DEPTH_STENCIL: i32 = 8;

/// Checks whether a clear-only pass `c` can fold into render pass `r`'s load action.
///
/// Returns the color attachment index (`0..8`), [`CLEAR_DEPTH_STENCIL`] (`8`) for depth/stencil,
/// or `-1` when `c` cannot fold into `r`.
#[must_use]
pub fn clear_slot(c: &ClearEncoderData, r: &RenderEncoderData) -> i32 {
    if c.width != r.render_target_width
        || c.height != r.render_target_height
        || c.array_length != u32::from(r.render_target_array_length)
    {
        return -1;
    }
    if c.clear_dsv != 0 {
        let depth_mismatch = (c.clear_dsv & 1) != 0 && r.depth.attachment != c.attachment;
        let stencil_mismatch = (c.clear_dsv & 2) != 0 && r.stencil.attachment != c.attachment;
        return if depth_mismatch || stencil_mismatch {
            -1
        } else {
            CLEAR_DEPTH_STENCIL
        };
    }
    for (i, color) in r.colors.iter().enumerate() {
        if color.attachment.is_some()
            && color.attachment == c.attachment
            && color.depth_plane == c.depth_plane
        {
            return i32::try_from(i).unwrap_or(-1);
        }
    }
    -1
}

/// Blit pass encoder record (`BlitEncoderData`, `d3d12_command_encoder.hpp:160`).
#[repr(C)]
pub struct BlitEncoderData {
    pub base: EncoderData,
    pub cmd_head: WmtcmdBlitNop,
    pub cmd_tail: *mut WmtcmdBase,
}

/// Compute pass encoder record (`ComputeEncoderData`, `d3d12_command_encoder.hpp:165`).
#[repr(C)]
pub struct ComputeEncoderData {
    pub base: EncoderData,
    pub cmd_head: WmtcmdComputeNop,
    pub cmd_tail: *mut WmtcmdBase,
}

/// MSAA resolve pass encoder record (`ResolveEncoderData`, `d3d12_command_encoder.hpp:169`).
#[repr(C)]
pub struct ResolveEncoderData {
    pub base: EncoderData,
    pub src: Option<TextureViewRef>,
    pub dst: Option<TextureViewRef>,
}

/// `MetalFX` temporal upscale encoder record (`d3d12_command_encoder.hpp:178`).
#[repr(C)]
pub struct TemporalUpscaleEncoderData {
    pub base: EncoderData,
    pub scaler: Option<Arc<TemporalScaler>>,
    pub color: Option<TextureViewRef>,
    pub depth: Option<TextureViewRef>,
    pub motion: Option<TextureViewRef>,
    pub exposure: Option<TextureViewRef>,
    pub reactive: Option<TextureViewRef>,
    pub output: Option<TextureViewRef>,
    pub depth_scratch: Option<Arc<Texture>>,
    pub motion_scratch: Option<Arc<Texture>>,
    pub output_scratch: Option<Arc<Texture>>,
    pub props: WMTFXTemporalScalerProps,
}

/// CPU-side timestamp resolve deferred until command-buffer completion (`d3d12_device.hpp:56-66`).
pub struct TimestampResolve {
    pub samples: u64,
    pub start: u32,
    pub count: u32,
    pub dst: *mut c_void,
    pub aliases: Vec<(u32, u32)>,
    pub values: *const u64,
}

/// Reason a barrier call blocks render-pass merging (`d3d12_device.hpp:81-93`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BarrierOpacity {
    UavState = 0,
    UavBarrier = 1,
    Aliasing = 2,
    SplitEnd = 3,
    Unknown = 4,
    Simultaneous = 5,
    Common = 6,
}

impl BarrierOpacity {
    /// Total number of opacity categories (`kOpaqueKinds`).
    pub const KINDS: usize = 7;
    /// First category that blocks merging regardless of whether passes write UAVs.
    pub const ALWAYS: Self = Self::SplitEnd;
}

/// Closed command-list fields read by the command queue (`d3d12_device.hpp:68-95`).
pub struct Submission {
    pub entry: *mut EncoderData,
    pub encoder_count: usize,
    pub barrier_count: u32,
    pub timestamp_resolves: Vec<TimestampResolve>,
    pub timestamps: bool,
    pub custom_resolves: bool,
    pub barrier_names: Vec<(u32, *const c_void)>,
    pub barrier_opaque: Vec<(u32, BarrierOpacity)>,
}

impl Submission {
    /// Creates an empty submission in the closed state (`encoder_count = 0`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entry: ptr::null_mut(),
            encoder_count: 0,
            barrier_count: 0,
            timestamp_resolves: Vec::new(),
            timestamps: false,
            custom_resolves: false,
            barrier_names: Vec::new(),
            barrier_opaque: Vec::new(),
        }
    }
}

impl Default for Submission {
    fn default() -> Self {
        Self::new()
    }
}

/// Drops the concrete encoder struct headed by `encoder` in place (`CommandAllocator::Reset`).
///
/// # Safety
///
/// `encoder` must point to a live encoder node initialized with the concrete struct matching
/// `(*encoder).kind`, and must not be accessed after this call.
pub unsafe fn drop_encoder_in_place(encoder: *mut EncoderData) {
    // SAFETY: `encoder` is a valid header pointer.
    let kind = unsafe { (*encoder).kind };
    match kind {
        EncoderType::Null => {
            // SAFETY: `encoder` is a `EncoderData`.
            unsafe { ptr::drop_in_place(encoder) };
        }
        EncoderType::Clear => {
            // SAFETY: `kind == Clear` guarantees `encoder` heads a `ClearEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<ClearEncoderData>()) };
        }
        EncoderType::Render => {
            // SAFETY: `kind == Render` guarantees `encoder` heads a `RenderEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<RenderEncoderData>()) };
        }
        EncoderType::Blit => {
            // SAFETY: `kind == Blit` guarantees `encoder` heads a `BlitEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<BlitEncoderData>()) };
        }
        EncoderType::Compute => {
            // SAFETY: `kind == Compute` guarantees `encoder` heads a `ComputeEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<ComputeEncoderData>()) };
        }
        EncoderType::Resolve => {
            // SAFETY: `kind == Resolve` guarantees `encoder` heads a `ResolveEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<ResolveEncoderData>()) };
        }
        EncoderType::TemporalUpscale => {
            // SAFETY: `kind == TemporalUpscale` guarantees `encoder` heads a
            // `TemporalUpscaleEncoderData`.
            unsafe { ptr::drop_in_place(encoder.cast::<TemporalUpscaleEncoderData>()) };
        }
    }
}
