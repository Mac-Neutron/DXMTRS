//! winemetal's wire: the structures its Unix calls carry, their constants and the slot numbers.
//!
//! `#[repr(C)]` mirrors of every structure of `winemetal_thunks.h` and `airconv_thunks.h` (the
//! 32-bit `*_params32` and the VM guest's `unixcall_wmtmemory` among them) and of the
//! `winemetal.h` structures they carry, for winemetal's Unix side and its PE side. `layouts.rs`
//! asserts each one's size and offsets against clang's layout of the C on every target the two
//! build for, and each constant against the C's value.
//!
//! Names take Rust casing: a `snake_case` structure name becomes `CamelCase` and a `WMT` one stays,
//! fields are `snake_case` (`type` is `r#type`), constants `SCREAMING_SNAKE_CASE`
//! (`WMTPixelFormatBGRX8Unorm` is [`WMT_PIXEL_FORMAT_BGRX8_UNORM`]). A field has the C's storage
//! type, since the bytes come from another process: an enum is its integer, a `bool` a `u8`, a
//! pointer of `winemetal.h` a [`MemPtr`]. A structure's bitfields are one integer, `bits`, read
//! through `<FIELD>_SHIFT`, `<FIELD>_WIDTH` and a getter per field.

#![no_std]

use core::ffi::{c_char, c_void};

mod layouts;
pub mod slot;
#[cfg(test)]
mod tests;

/// A pointer in a structure of `winemetal.h`: `WMTMemoryPointer` and `WMTConstMemoryPointer`.
///
/// Eight bytes on every target: a 32-bit process writes its address and a high half of 0.
#[cfg(target_pointer_width = "64")]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MemPtr {
    pub ptr: u64,
}

/// A pointer in a structure of `winemetal.h`: `WMTMemoryPointer` and `WMTConstMemoryPointer`.
///
/// Eight bytes on every target: a 32-bit process writes its address and a high half of 0.
#[cfg(target_pointer_width = "32")]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MemPtr {
    pub ptr: u32,
    pub high: u32,
}

/// The `NULL_OBJECT_HANDLE` of `winemetal.h`.
pub const NULL_OBJECT_HANDLE: u64 = 0;

/// The `AIRCONV_VERSION` of `airconv_public.h`.
pub const AIRCONV_VERSION: u32 = 26;

/// `ORIGINAL_FORMAT`: a pixel format without the swizzle in its high bits.
#[must_use]
pub const fn original_format(format: u32) -> u32 {
    format & !WMT_PIXEL_FORMAT_CUSTOM_SWIZZLE
}

/// `GET_SWIZZLE_RED`: the swizzle a pixel format gives red, a `WMT_TEXTURE_SWIZZLE_*`.
#[must_use]
pub const fn get_swizzle_red(format: u32) -> u32 {
    decode_format_component_swizzle(format, 0, 2)
}

/// `GET_SWIZZLE_GREEN`: the swizzle a pixel format gives green, a `WMT_TEXTURE_SWIZZLE_*`.
#[must_use]
pub const fn get_swizzle_green(format: u32) -> u32 {
    decode_format_component_swizzle(format, 1, 3)
}

/// `GET_SWIZZLE_BLUE`: the swizzle a pixel format gives blue, a `WMT_TEXTURE_SWIZZLE_*`.
#[must_use]
pub const fn get_swizzle_blue(format: u32) -> u32 {
    decode_format_component_swizzle(format, 2, 4)
}

/// `GET_SWIZZLE_ALPHA`: the swizzle a pixel format gives alpha, a `WMT_TEXTURE_SWIZZLE_*`.
#[must_use]
pub const fn get_swizzle_alpha(format: u32) -> u32 {
    decode_format_component_swizzle(format, 3, 5)
}

/// `ENCODE_FORMAT_SWIZZLE`: `base_format` with the swizzles `r`, `g`, `b` and `a` in its high bits.
#[must_use]
pub const fn encode_format_swizzle(base_format: u32, r: u8, g: u8, b: u8, a: u8) -> u32 {
    (base_format & 0xfff)
        | (encode_format_component_offset(r, 2) << 21)
        | (encode_format_component_offset(g, 3) << 18)
        | (encode_format_component_offset(b, 4) << 15)
        | (encode_format_component_offset(a, 5) << 12)
}

/// `DECODE_FORMAT_COMPONENT_SWIZZLE`: the swizzle `format` makes of `base` for a component.
///
/// `component` is 0 to 3 (red to alpha) and `base` a `WMT_TEXTURE_SWIZZLE_*`: the component's
/// identity swizzle for the `GET_SWIZZLE_*` getters, a texture view's requested swizzle for
/// winemetal's views.
#[must_use]
pub const fn decode_format_component_swizzle(format: u32, component: u32, base: u32) -> u32 {
    (base + ((format >> (21 - 3 * component)) & 0x7)) % 6
}

/// `ENCODE_FORMAT_COMPONENT_OFFSET`, for a component's identity swizzle `base`.
const fn encode_format_component_offset(target: u8, base: u32) -> u32 {
    (target as u32 + (6 - base)) % 6
}

/// The `width` bits of `bits` from bit `shift` (1 to 32 bits).
const fn bitfield(bits: u32, shift: u32, width: u32) -> u32 {
    (bits >> shift) & (u32::MAX >> (32 - width))
}

// winemetal.h

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTBufferInfo {
    pub length: u64,
    pub options: u64,
    pub memory: MemPtr,
    pub gpu_address: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTSamplerInfo {
    pub min_filter: u8,
    pub mag_filter: u8,
    pub mip_filter: u8,
    pub r_address_mode: u8,
    pub s_address_mode: u8,
    pub t_address_mode: u8,
    pub border_color: u8,
    pub compare_function: u8,
    pub lod_min_clamp: f32,
    pub lod_max_clamp: f32,
    pub max_anisotroy: u32,
    pub normalized_coords: u8,
    pub lod_average: u8,
    pub support_argument_buffers: u8,
    pub gpu_resource_id: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTStencilInfo {
    pub enabled: u8,
    pub depth_stencil_pass_op: u8,
    pub stencil_fail_op: u8,
    pub depth_fail_op: u8,
    pub stencil_compare_function: u8,
    pub write_mask: u8,
    pub read_mask: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTDepthStencilInfo {
    pub depth_compare_function: u8,
    pub depth_write_enabled: u8,
    pub front_stencil: WMTStencilInfo,
    pub back_stencil: WMTStencilInfo,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTTextureSwizzleChannels {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTTextureInfo {
    pub pixel_format: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_length: u32,
    /// `type`, `mipmap_level_count`, `sample_count` and `usage`, eight bits each.
    pub bits: u32,
    pub options: u64,
    pub flags: u32,
    pub mach_port: u32,
    pub gpu_resource_id: u64,
}

impl WMTTextureInfo {
    pub const TYPE_SHIFT: u32 = 0;
    pub const TYPE_WIDTH: u32 = 8;
    pub const MIPMAP_LEVEL_COUNT_SHIFT: u32 = 8;
    pub const MIPMAP_LEVEL_COUNT_WIDTH: u32 = 8;
    pub const SAMPLE_COUNT_SHIFT: u32 = 16;
    pub const SAMPLE_COUNT_WIDTH: u32 = 8;
    pub const USAGE_SHIFT: u32 = 24;
    pub const USAGE_WIDTH: u32 = 8;

    #[must_use]
    pub const fn r#type(&self) -> u32 {
        bitfield(self.bits, Self::TYPE_SHIFT, Self::TYPE_WIDTH)
    }

    #[must_use]
    pub const fn mipmap_level_count(&self) -> u32 {
        bitfield(
            self.bits,
            Self::MIPMAP_LEVEL_COUNT_SHIFT,
            Self::MIPMAP_LEVEL_COUNT_WIDTH,
        )
    }

    #[must_use]
    pub const fn sample_count(&self) -> u32 {
        bitfield(
            self.bits,
            Self::SAMPLE_COUNT_SHIFT,
            Self::SAMPLE_COUNT_WIDTH,
        )
    }

    #[must_use]
    pub const fn usage(&self) -> u32 {
        bitfield(self.bits, Self::USAGE_SHIFT, Self::USAGE_WIDTH)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTComputePipelineInfo {
    pub compute_function: u64,
    pub binary_archives_for_lookup: MemPtr,
    pub binary_archive_for_serialization: u64,
    pub num_binary_archives_for_lookup: u8,
    pub fail_on_binary_archive_miss: u8,
    pub support_indirect_command_buffers: u8,
    pub tgsize_is_multiple_of_sgwidth: u8,
    pub immutable_buffers: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTClearColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTColorAttachmentInfo {
    pub texture: u64,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_color: WMTClearColor,
    pub resolve_texture: u64,
    pub resolve_level: u16,
    pub resolve_slice: u16,
    pub resolve_depth_plane: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTDepthAttachmentInfo {
    pub texture: u64,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_depth: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTStencilAttachmentInfo {
    pub texture: u64,
    pub load_action: u32,
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
    pub clear_stencil: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTRenderPassSampleBufferInfo {
    pub sample_buffer: u64,
    pub end_of_fragment_sample_index: u64,
    pub start_of_vertex_sample_index: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTRenderPassInfo {
    pub colors: [WMTColorAttachmentInfo; 8],
    pub depth: WMTDepthAttachmentInfo,
    pub stencil: WMTStencilAttachmentInfo,
    pub default_raster_sample_count: u8,
    pub render_target_array_length: u8,
    pub tile_width: u8,
    pub tile_height: u8,
    pub render_target_height: u32,
    pub render_target_width: u32,
    pub visibility_accumulate: u8,
    pub visibility_buffer: u64,
    pub sample_buffers: [WMTRenderPassSampleBufferInfo; 4],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTColorAttachmentBlendInfo {
    pub pixel_format: u32,
    pub rgb_blend_operation: u8,
    pub alpha_blend_operation: u8,
    pub src_rgb_blend_factor: u8,
    pub dst_rgb_blend_factor: u8,
    pub src_alpha_blend_factor: u8,
    pub dst_alpha_blend_factor: u8,
    pub write_mask: u8,
    pub blending_enabled: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTRenderPipelineBlendInfo {
    pub colors: [WMTColorAttachmentBlendInfo; 8],
    pub alpha_to_coverage_enabled: u8,
    pub logic_operation_enabled: u8,
    pub logic_operation: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTRenderPipelineInfo {
    pub colors: [WMTColorAttachmentBlendInfo; 8],
    pub alpha_to_coverage_enabled: u8,
    pub logic_operation_enabled: u8,
    pub logic_operation: u8,
    pub rasterization_enabled: u8,
    pub raster_sample_count: u8,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
    pub vertex_function: u64,
    pub fragment_function: u64,
    pub immutable_vertex_buffers: u32,
    pub immutable_fragment_buffers: u32,
    pub input_primitive_topology: u32,
    pub tessellation_partition_mode: u8,
    pub max_tessellation_factor: u8,
    pub tessellation_output_winding_order: u8,
    pub tessellation_factor_step: u8,
    pub binary_archive_for_serialization: u64,
    pub binary_archives_for_lookup: MemPtr,
    pub num_binary_archives_for_lookup: u8,
    pub fail_on_binary_archive_miss: u8,
    pub support_indirect_command_buffers: u8,
    pub padding: [u8; 5],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTMeshRenderPipelineInfo {
    pub colors: [WMTColorAttachmentBlendInfo; 8],
    pub alpha_to_coverage_enabled: u8,
    pub logic_operation_enabled: u8,
    pub logic_operation: u8,
    pub rasterization_enabled: u8,
    pub raster_sample_count: u8,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
    pub object_function: u64,
    pub mesh_function: u64,
    pub fragment_function: u64,
    pub immutable_object_buffers: u32,
    pub immutable_mesh_buffers: u32,
    pub immutable_fragment_buffers: u32,
    pub payload_memory_length: u16,
    pub mesh_tgsize_is_multiple_of_sgwidth: u8,
    pub object_tgsize_is_multiple_of_sgwidth: u8,
    pub binary_archive_for_serialization: u64,
    pub binary_archives_for_lookup: MemPtr,
    pub num_binary_archives_for_lookup: u8,
    pub fail_on_binary_archive_miss: u8,
    pub support_indirect_command_buffers: u8,
    pub padding: [u8; 5],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTVertexAttributeInfo {
    pub format: u32,
    pub offset: u32,
    pub buffer_index: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTVertexBufferLayoutInfo {
    pub stride: u32,
    pub step_function: u32,
    pub step_rate: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTVertexDescriptorInfo {
    pub attributes: [WMTVertexAttributeInfo; 31],
    pub layouts: [WMTVertexBufferLayoutInfo; 31],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTSize {
    pub width: u64,
    pub height: u64,
    pub depth: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTOrigin {
    pub x: u64,
    pub y: u64,
    pub z: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBase {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitNop {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromBufferToBuffer {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub src_offset: u64,
    pub dst: u64,
    pub dst_offset: u64,
    pub copy_length: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromBufferToTexture {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub src_offset: u64,
    pub bytes_per_row: u32,
    pub bytes_per_image: u32,
    pub size: WMTSize,
    pub dst: u64,
    pub slice: u32,
    pub level: u32,
    pub origin: WMTOrigin,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromBufferToTextureWithblitoption {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub src_offset: u64,
    pub bytes_per_row: u32,
    pub bytes_per_image: u32,
    pub size: WMTSize,
    pub dst: u64,
    pub slice: u32,
    pub level: u16,
    pub options: u16,
    pub origin: WMTOrigin,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromTextureToTexture {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub src_slice: u32,
    pub src_level: u32,
    pub src_origin: WMTOrigin,
    pub src_size: WMTSize,
    pub dst: u64,
    pub dst_slice: u32,
    pub dst_level: u32,
    pub dst_origin: WMTOrigin,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromTextureToBuffer {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub slice: u32,
    pub level: u32,
    pub origin: WMTOrigin,
    pub size: WMTSize,
    pub dst: u64,
    pub offset: u64,
    pub bytes_per_row: u32,
    pub bytes_per_image: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyFromTextureToBufferWithblitoption {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub slice: u32,
    pub level: u16,
    pub options: u16,
    pub origin: WMTOrigin,
    pub size: WMTSize,
    pub dst: u64,
    pub offset: u64,
    pub bytes_per_row: u32,
    pub bytes_per_image: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitGenerateMipmaps {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub texture: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitFenceOp {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub fence: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitFillbuffer {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub buffer: u64,
    pub offset: u64,
    pub length: u64,
    pub value: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitResolvecounters {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub sample_buffer: u64,
    pub start: u32,
    pub len: u32,
    pub dst_buffer: u64,
    pub dst_offset: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitResetcommands {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub indirect_command_buffer: u64,
    pub location: u32,
    pub length: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdBlitCopyTexture {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub src: u64,
    pub dst: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeNop {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeDispatch {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub size: WMTSize,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeDispatchIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeSetpso {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub pso: u64,
    pub threadgroup_size: WMTSize,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeSetbuffer {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub buffer: u64,
    pub offset: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeSetbufferoffset {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub offset: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeUseresource {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub resource: u64,
    pub usage: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeSetbytes {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub bytes: MemPtr,
    pub length: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeSettexture {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub texture: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeFenceOp {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub fence: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeMemoryBarrier {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub scope: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdComputeExecutecommands {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub indirect_command_buffer: u64,
    pub location: u32,
    pub length: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderNop {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderUseresource {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub resource: u64,
    pub usage: u32,
    pub stages: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetbuffer {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub buffer: u64,
    pub offset: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetbytes {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub bytes: MemPtr,
    pub length: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSettexture {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub texture: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetbufferoffset {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub offset: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetrasterizerstate {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub fill_mode: u8,
    pub cull_mode: u8,
    pub depth_clip_mode: u8,
    pub winding: u8,
    pub depth_bias: f32,
    pub scole_scale: f32,
    pub depth_bias_clamp: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetpso {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub pso: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetvisibilitymode {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub offset: u64,
    pub mode: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDraw {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub primitive_type: u8,
    pub vertex_start: u64,
    pub vertex_count: u64,
    pub instance_count: u32,
    pub base_instance: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDrawIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub primitive_type: u8,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDrawIndexed {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub primitive_type: u8,
    pub index_type: u8,
    pub index_count: u64,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub instance_count: u32,
    pub base_vertex: i32,
    pub base_instance: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDrawIndexedIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub primitive_type: u8,
    pub index_type: u8,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDrawMeshthreadgroups {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub threadgroup_per_grid: WMTSize,
    pub object_threadgroup_size: WMTSize,
    pub mesh_threadgroup_size: WMTSize,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDrawMeshthreadgroupsIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
    pub object_threadgroup_size: WMTSize,
    pub mesh_threadgroup_size: WMTSize,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderMemoryBarrier {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub scope: u8,
    pub stages_before: u8,
    pub stages_after: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTViewport {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub znear: f64,
    pub zfar: f64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetviewports {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub viewports: MemPtr,
    pub viewport_count: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetviewport {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub viewport: WMTViewport,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTScissorRect {
    pub x: u64,
    pub y: u64,
    pub width: u64,
    pub height: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetscissorrects {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub scissor_rects: MemPtr,
    pub rect_count: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetscissorrect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub scissor_rect: WMTScissorRect,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetdsso {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub dsso: u64,
    pub stencil_ref: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetdepthstencilstate {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub depth_stencil_state: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetsampler {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub sampler: u64,
    pub index: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetblendcolor {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
    pub stencil_ref: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderSetstencilref {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub stencil_ref: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtGeometryDraw {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub draw_arguments_offset: u64,
    pub warp_count: u32,
    pub instance_count: u32,
    pub vertex_per_warp: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtGeometryDrawIndexed {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub draw_arguments_offset: u64,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub warp_count: u32,
    pub instance_count: u32,
    pub vertex_per_warp: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtGeometryDrawIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub imm_draw_arguments: u64,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
    pub dispatch_args_buffer: u64,
    pub dispatch_args_offset: u64,
    pub vertex_per_warp: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtGeometryDrawIndexedIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub imm_draw_arguments: u64,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
    pub dispatch_args_buffer: u64,
    pub dispatch_args_offset: u64,
    pub vertex_per_warp: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderFenceOp {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub fence: u64,
    pub stages: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtTessellationMeshDraw {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub draw_arguments_offset: u64,
    pub instance_count: u32,
    pub threads_per_patch: u32,
    pub patch_per_group: u32,
    pub patch_per_mesh_instance: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtTessellationMeshDrawIndexed {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub draw_arguments_offset: u64,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub instance_count: u32,
    pub threads_per_patch: u32,
    pub patch_per_group: u32,
    pub patch_per_mesh_instance: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtTessellationMeshDrawIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub imm_draw_arguments: u64,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
    pub dispatch_args_buffer: u64,
    pub dispatch_args_offset: u64,
    pub threads_per_patch: u32,
    pub patch_per_group: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub imm_draw_arguments: u64,
    pub indirect_args_buffer: u64,
    pub indirect_args_offset: u64,
    pub dispatch_args_buffer: u64,
    pub dispatch_args_offset: u64,
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub threads_per_patch: u32,
    pub patch_per_group: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderDispatchThreadsPerTile {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WmtcmdRenderExecutecommands {
    pub r#type: u16,
    pub reserved: [u16; 3],
    pub next: MemPtr,
    pub indirect_command_buffer: u64,
    pub location: u32,
    pub length: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTCaptureInfo {
    pub capture_object: u64,
    pub output_url: MemPtr,
    pub destination: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFXTemporalScalerInfo {
    pub color_format: u32,
    pub output_format: u32,
    pub depth_format: u32,
    pub motion_format: u32,
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub input_content_min_scale: f32,
    pub input_content_max_scale: f32,
    pub auto_exposure: u8,
    pub input_content_properties_enabled: u8,
    pub requires_synchronous_initialization: u8,
    pub reactive_mask_enabled: u8,
    pub reactive_mask_format: u32,
    pub output_resolution_motion_vectors: u8,
    pub jittered_motion_vectors: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFXSpatialScalerInfo {
    pub color_format: u32,
    pub output_format: u32,
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFXTemporalScalerProps {
    pub input_content_width: u32,
    pub input_content_height: u32,
    pub reset: u8,
    pub depth_reversed: u8,
    pub motion_vector_scale_x: f32,
    pub motion_vector_scale_y: f32,
    pub jitter_offset_x: f32,
    pub jitter_offset_y: f32,
    pub pre_exposure: f32,
    pub color_content_offset_x: u32,
    pub color_content_offset_y: u32,
    pub depth_content_offset_x: u32,
    pub depth_content_offset_y: u32,
    pub motion_content_offset_x: u32,
    pub motion_content_offset_y: u32,
    pub reactive_mask_content_offset_x: u32,
    pub reactive_mask_content_offset_y: u32,
    pub output_offset_x: u32,
    pub output_offset_y: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFXFrameInterpolatorInfo {
    pub color_format: u32,
    pub output_format: u32,
    pub depth_format: u32,
    pub motion_format: u32,
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub scaler: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFXFrameInterpolatorProps {
    pub motion_vector_scale_x: f32,
    pub motion_vector_scale_y: f32,
    pub delta_time: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub field_of_view: f32,
    pub aspect_ratio: f32,
    pub jitter_offset_x: f32,
    pub jitter_offset_y: f32,
    pub reset: u8,
    pub depth_reversed: u8,
    pub ui_composited: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTLayerProps {
    pub device: u64,
    pub contents_scale: f64,
    pub drawable_width: f64,
    pub drawable_height: f64,
    pub opaque: u8,
    pub display_sync_enabled: u8,
    pub framebuffer_only: u8,
    pub pixel_format: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTDisplayDescription {
    pub red_primaries: [f32; 2],
    pub blue_primaries: [f32; 2],
    pub green_primaries: [f32; 2],
    pub white_points: [f32; 2],
    pub maximum_edr_color_component_value: f32,
    pub maximum_potential_edr_color_component_value: f32,
    pub maximum_reference_edr_color_component_value: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTEDRValue {
    pub maximum_edr_color_component_value: f32,
    pub maximum_potential_edr_color_component_value: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTFunctionConstant {
    pub data: MemPtr,
    pub r#type: u16,
    pub index: u16,
    pub reserved: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTHDRMetadata {
    pub red_primary: [u16; 2],
    pub green_primary: [u16; 2],
    pub blue_primary: [u16; 2],
    pub white_point: [u16; 2],
    pub max_mastering_luminance: u32,
    pub min_mastering_luminance: u32,
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTSampleBufferAttachmentInfo {
    pub sample_buffer: u64,
    pub start_of_encoder_sample_index: u64,
    pub end_of_encoder_sample_index: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTTileRenderPipelineInfo {
    pub color_formats: [u32; 8],
    pub tile_function: u64,
    pub immutable_tile_buffers: u32,
    pub raster_sample_count: u8,
    pub tgsize_matches_tile_size: u8,
    pub binary_archive_for_serialization: u64,
    pub binary_archives_for_lookup: MemPtr,
    pub num_binary_archives_for_lookup: u8,
    pub fail_on_binary_archive_miss: u8,
    pub padding: [u8; 6],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTHeapInfo {
    pub size: u64,
    pub options: u64,
    pub r#type: u32,
    pub sparse_page_size: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTSizeAndAlign {
    pub size: u32,
    pub align: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTIndirectCommandBufferInfo {
    pub r#type: u32,
    /// The eleven `inherit_*` and `support_*` flags, one bit each, then 21 padding bits.
    pub bits: u32,
    pub max_vertex_buffer_binding: u8,
    pub max_fragment_buffer_binding: u8,
    pub max_kernel_buffer_binding: u8,
    pub max_mesh_buffer_binding: u8,
    pub max_object_buffer_binding: u8,
    pub max_object_threadgroup_memory_binding: u8,
    pub max_kernel_threadgroup_memory_binding: u8,
    pub padding: u8,
    pub gpu_resource_id: u64,
}

impl WMTIndirectCommandBufferInfo {
    pub const INHERIT_BUFFERS_SHIFT: u32 = 0;
    pub const INHERIT_BUFFERS_WIDTH: u32 = 1;
    pub const INHERIT_PSO_SHIFT: u32 = 1;
    pub const INHERIT_PSO_WIDTH: u32 = 1;
    pub const INHERIT_CULL_MODE_SHIFT: u32 = 2;
    pub const INHERIT_CULL_MODE_WIDTH: u32 = 1;
    pub const INHERIT_DEPTH_BIAS_SHIFT: u32 = 3;
    pub const INHERIT_DEPTH_BIAS_WIDTH: u32 = 1;
    pub const INHERIT_DEPTH_CLIP_MODE_SHIFT: u32 = 4;
    pub const INHERIT_DEPTH_CLIP_MODE_WIDTH: u32 = 1;
    pub const INHERIT_DEPTH_STENCIL_STATE_SHIFT: u32 = 5;
    pub const INHERIT_DEPTH_STENCIL_STATE_WIDTH: u32 = 1;
    pub const INHERIT_FRONT_FACING_SHIFT: u32 = 6;
    pub const INHERIT_FRONT_FACING_WIDTH: u32 = 1;
    pub const INHERIT_FILL_MODE_SHIFT: u32 = 7;
    pub const INHERIT_FILL_MODE_WIDTH: u32 = 1;
    pub const SUPPORT_RAY_TRACING_SHIFT: u32 = 8;
    pub const SUPPORT_RAY_TRACING_WIDTH: u32 = 1;
    pub const SUPPORT_DYNAMIC_ATTRIBUTE_STRIDE_SHIFT: u32 = 9;
    pub const SUPPORT_DYNAMIC_ATTRIBUTE_STRIDE_WIDTH: u32 = 1;
    pub const SUPPORT_COLOR_ATTACHMENT_MAPPING_SHIFT: u32 = 10;
    pub const SUPPORT_COLOR_ATTACHMENT_MAPPING_WIDTH: u32 = 1;
    pub const PADDING_BITS_SHIFT: u32 = 11;
    pub const PADDING_BITS_WIDTH: u32 = 21;

    #[must_use]
    pub const fn inherit_buffers(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_BUFFERS_SHIFT,
            Self::INHERIT_BUFFERS_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_pso(&self) -> u32 {
        bitfield(self.bits, Self::INHERIT_PSO_SHIFT, Self::INHERIT_PSO_WIDTH)
    }

    #[must_use]
    pub const fn inherit_cull_mode(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_CULL_MODE_SHIFT,
            Self::INHERIT_CULL_MODE_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_depth_bias(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_DEPTH_BIAS_SHIFT,
            Self::INHERIT_DEPTH_BIAS_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_depth_clip_mode(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_DEPTH_CLIP_MODE_SHIFT,
            Self::INHERIT_DEPTH_CLIP_MODE_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_depth_stencil_state(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_DEPTH_STENCIL_STATE_SHIFT,
            Self::INHERIT_DEPTH_STENCIL_STATE_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_front_facing(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_FRONT_FACING_SHIFT,
            Self::INHERIT_FRONT_FACING_WIDTH,
        )
    }

    #[must_use]
    pub const fn inherit_fill_mode(&self) -> u32 {
        bitfield(
            self.bits,
            Self::INHERIT_FILL_MODE_SHIFT,
            Self::INHERIT_FILL_MODE_WIDTH,
        )
    }

    #[must_use]
    pub const fn support_ray_tracing(&self) -> u32 {
        bitfield(
            self.bits,
            Self::SUPPORT_RAY_TRACING_SHIFT,
            Self::SUPPORT_RAY_TRACING_WIDTH,
        )
    }

    #[must_use]
    pub const fn support_dynamic_attribute_stride(&self) -> u32 {
        bitfield(
            self.bits,
            Self::SUPPORT_DYNAMIC_ATTRIBUTE_STRIDE_SHIFT,
            Self::SUPPORT_DYNAMIC_ATTRIBUTE_STRIDE_WIDTH,
        )
    }

    #[must_use]
    pub const fn support_color_attachment_mapping(&self) -> u32 {
        bitfield(
            self.bits,
            Self::SUPPORT_COLOR_ATTACHMENT_MAPPING_SHIFT,
            Self::SUPPORT_COLOR_ATTACHMENT_MAPPING_WIDTH,
        )
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct WMTCompileOptions {
    pub language_version: u64,
    pub preserve_invariance: u8,
    pub math_mode: u8,
    pub padding: [u8; 6],
}

// winemetal_thunks.h

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjRet {
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjNoret {
    pub handle: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjRet {
    pub handle: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjUint64ObjRet {
    pub handle: u64,
    pub arg: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjUint64Ret {
    pub handle: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjUint64Noret {
    pub handle: u64,
    pub arg: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjPtrNoret {
    pub handle: u64,
    pub arg: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjConstptrNoret {
    pub handle: u64,
    pub arg: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjUint64Noret {
    pub handle: u64,
    pub arg0: u64,
    pub arg1: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjUint64Uint64Ret {
    pub handle: u64,
    pub arg: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallNsstringGetcstring {
    pub str: u64,
    pub buffer_ptr: u64,
    pub max_length: u64,
    pub encoding: u64,
    pub ret: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewbuffer {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewsamplerstate {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewdepthstencilstate {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewtexture {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlbufferNewtexture {
    pub buffer: u64,
    pub info: MemPtr,
    pub offset: u64,
    pub bytes_per_row: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtltextureNewtextureview {
    pub texture: u64,
    pub format: u32,
    pub texture_type: u32,
    pub level_start: u16,
    pub level_count: u16,
    pub slice_start: u16,
    pub slice_count: u16,
    pub swizzle: WMTTextureSwizzleChannels,
    pub ret: u64,
    pub gpu_resource_id: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewlibrary {
    pub device: u64,
    pub data: u64,
    pub ret_error: u64,
    pub ret_library: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewlibrarywithsource {
    pub device: u64,
    pub source: MemPtr,
    pub length: u64,
    pub ret_error: u64,
    pub ret_library: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewlibrarywithsourceOptions {
    pub device: u64,
    pub source: MemPtr,
    pub length: u64,
    pub options: MemPtr,
    pub ret_error: u64,
    pub ret_library: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewcomputepso {
    pub device: u64,
    pub info: MemPtr,
    pub ret_error: u64,
    pub ret_pso: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewrenderpso {
    pub device: u64,
    pub info: MemPtr,
    pub ret_error: u64,
    pub ret_pso: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewrenderpsoVertex {
    pub device: u64,
    pub info: MemPtr,
    pub vertex: MemPtr,
    pub ret_error: u64,
    pub ret_pso: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewmeshrenderpso {
    pub device: u64,
    pub info: MemPtr,
    pub ret_error: u64,
    pub ret_pso: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjCmdNoret {
    pub encoder: u64,
    pub cmd_head: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtltextureReplaceregion {
    pub texture: u64,
    pub origin: WMTOrigin,
    pub size: WMTSize,
    pub level: u64,
    pub slice: u64,
    pub data: MemPtr,
    pub bytes_per_row: u64,
    pub bytes_per_image: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjNoret {
    pub handle: u64,
    pub arg: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjObjNoret {
    pub handle: u64,
    pub arg0: u64,
    pub arg1: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjObjUint64Ret {
    pub handle: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGenericObjObjDoubleNoret {
    pub handle: u64,
    pub arg0: u64,
    pub arg1: f64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcapturemanagerStartcapture {
    pub capture_manager: u64,
    pub info: MemPtr,
    pub ret: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewfxtemporalscaler {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewfxspatialscaler {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcommandbufferTemporalScale {
    pub cmdbuf: u64,
    pub scaler: u64,
    pub color: u64,
    pub output: u64,
    pub depth: u64,
    pub motion: u64,
    pub exposure: u64,
    pub fence: u64,
    pub props: MemPtr,
    pub reactive_mask: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcommandbufferSpatialScale {
    pub cmdbuf: u64,
    pub scaler: u64,
    pub color: u64,
    pub output: u64,
    pub fence: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallNsstringString {
    pub buffer_ptr: MemPtr,
    pub encoding: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallCreateMetalViewFromHwnd {
    pub hwnd: u64,
    pub device: u64,
    pub ret_view: u64,
    pub ret_layer: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallEnumerate {
    pub enumerable: u64,
    pub start: u64,
    pub buffer_size: u64,
    pub buffer: MemPtr,
    pub ret_read: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtllibraryNewfunctionWithConstants {
    pub library: u64,
    pub name: MemPtr,
    pub constants: MemPtr,
    pub num_constants: u64,
    pub ret: u64,
    pub ret_error: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallQueryDisplaySetting {
    pub display_id: u64,
    pub colorspace: u64,
    pub hdr_metadata: MemPtr,
    pub ret: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallUpdateDisplaySetting {
    pub display_id: u64,
    pub colorspace: u64,
    pub hdr_metadata: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallQueryDisplaySettingForLayer {
    pub layer: u64,
    pub version: u64,
    pub colorspace: u64,
    pub hdr_metadata: MemPtr,
    pub edr_value: WMTEDRValue,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlbufferUpdatecontents {
    pub buffer: u64,
    pub offset: u64,
    pub data: MemPtr,
    pub length: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlsharedeventSetevent {
    pub shared_event: u64,
    pub event_handle: u64,
    pub shared_event_listener: u64,
    pub value: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlsharedeventSeteventatvalues {
    pub shared_event_listener: u64,
    pub shared_events: MemPtr,
    pub values: MemPtr,
    pub count: u32,
    pub needed: u32,
    pub event_handle: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlsharedeventCreatemachport {
    pub event: u64,
    pub ret_mach_port: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewsharedeventwithmachport {
    pub device: u64,
    pub mach_port: u32,
    pub ret_event: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallGetOsVersion {
    pub ret_major: u64,
    pub ret_minor: u64,
    pub ret_patch: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewbinaryarchive {
    pub device: u64,
    pub url: MemPtr,
    pub ret_archive: u64,
    pub ret_error: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlbinaryarchiveSerialize {
    pub archive: u64,
    pub url: MemPtr,
    pub ret_error: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallCacheAllocInit {
    pub path: MemPtr,
    pub version: u64,
    pub ret_cache: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallCacheGet {
    pub cache: u64,
    pub key: MemPtr,
    pub key_length: u64,
    pub ret_data: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallCacheSet {
    pub cache: u64,
    pub key: MemPtr,
    pub key_length: u64,
    pub value_data: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallSetmetalcachepath {
    pub path: MemPtr,
    pub ret_success: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallBootstrap {
    pub name: [u8; 128],
    pub mach_port: u32,
    pub reserved: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlsharedeventWaituntilsignaledvalue {
    pub event: u64,
    pub value: u64,
    pub timeout_ms: u64,
    pub ret_timeout: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcountersamplebufferNewtimestampbuffer {
    pub device: u64,
    pub sample_count: u32,
    pub shared: u32,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcountersamplebufferResolvecounterrange {
    pub sample_buffer: u64,
    pub start: u32,
    pub len: u32,
    pub data_out: MemPtr,
    pub data_length: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers {
    pub cmdbuf: u64,
    pub attachments: MemPtr,
    pub num_attachments: u64,
    pub concurrent: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceSampletimestamps {
    pub device: u64,
    pub cpu: u64,
    pub gpu: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallDispatchdataCopybytes {
    pub data: u64,
    pub buffer: MemPtr,
    pub capacity: u64,
    pub ret_size: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewfxframeinterpolator {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcommandbufferFrameInterpolate {
    pub cmdbuf: u64,
    pub interpolator: u64,
    pub color: u64,
    pub prev_color: u64,
    pub depth: u64,
    pub motion: u64,
    pub output: u64,
    pub fence: u64,
    pub props: MemPtr,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceFxtemporalscalerScale {
    pub device: u64,
    pub min_scale: f32,
    pub max_scale: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers {
    pub cmdbuf: u64,
    pub attachments: MemPtr,
    pub num_attachments: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewresidencyset {
    pub device: u64,
    pub init_capacity: u64,
    pub ret_error: u64,
    pub ret_set: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlresidencysetAddallocations {
    pub set: u64,
    pub allocations: MemPtr,
    pub count: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewheap {
    pub device: u64,
    pub info: MemPtr,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceHeapbuffersizeandalign {
    pub device: u64,
    pub length: u64,
    pub options: u64,
    pub ret_size: u32,
    pub ret_align: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceHeaptexturesizeandalign {
    pub device: u64,
    pub info: MemPtr,
    pub ret_size: u32,
    pub ret_align: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlheapNewbuffer {
    pub heap: u64,
    pub info: MemPtr,
    pub offset: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtlheapNewtexture {
    pub heap: u64,
    pub info: MemPtr,
    pub offset: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallMtldeviceNewicb {
    pub device: u64,
    pub info: MemPtr,
    pub max_count: u64,
    pub options: u64,
    pub ret: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UnixcallWmtmemory {
    pub length: u64,
    pub ptr: MemPtr,
}

// airconv_thunks.h

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50InitializeParams {
    pub bytecode: *const c_void,
    pub bytecode_size: usize,
    pub shader: *mut u64,
    pub reflection: *mut c_void,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50DestroyParams {
    pub shader: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileParams {
    pub shader: u64,
    pub args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetCompiledBitcodeParams {
    pub bitcode: u64,
    pub data_out: *mut c_void,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50DestroyBitcodeParams {
    pub bitcode: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetErrorMessageParams {
    pub error: u64,
    pub buffer: *mut c_char,
    pub buffer_size: usize,
    pub ret_size: usize,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50FreeErrorParams {
    pub error: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileGeometryPipelineVertexParams {
    pub vertex: u64,
    pub geometry: u64,
    pub vertex_args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileGeometryPipelineGeometryParams {
    pub vertex: u64,
    pub geometry: u64,
    pub geometry_args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineVertexParams {
    pub vertex: u64,
    pub hull: u64,
    pub vertex_args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineHullParams {
    pub vertex: u64,
    pub hull: u64,
    pub hull_args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineDomainParams {
    pub hull: u64,
    pub domain: u64,
    pub domain_args: *mut c_void,
    pub func_name: *const c_char,
    pub bitcode: *mut u64,
    pub error: *mut u64,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetArgumentsInfoParams {
    pub shader: u64,
    pub constant_buffers: *mut c_void,
    pub arguments: *mut c_void,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50InitializeParams32 {
    pub bytecode: u32,
    pub bytecode_size: u32,
    pub shader: u32,
    pub reflection: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileParams32 {
    pub shader: u64,
    pub args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetCompiledBitcodeParams32 {
    pub bitcode: u64,
    pub data_out: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetErrorMessageParams32 {
    pub error: u64,
    pub buffer: u32,
    pub buffer_size: u32,
    pub ret_size: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileGeometryPipelineVertexParams32 {
    pub vertex: u64,
    pub geometry: u64,
    pub vertex_args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileGeometryPipelineGeometryParams32 {
    pub vertex: u64,
    pub geometry: u64,
    pub geometry_args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineVertexParams32 {
    pub vertex: u64,
    pub hull: u64,
    pub vertex_args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineHullParams32 {
    pub vertex: u64,
    pub hull: u64,
    pub hull_args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50CompileTessellationPipelineDomainParams32 {
    pub hull: u64,
    pub domain: u64,
    pub domain_args: u32,
    pub func_name: u32,
    pub bitcode: u32,
    pub error: u32,
    pub ret: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sm50GetArgumentsInfoParams32 {
    pub shader: u64,
    pub constant_buffers: u32,
    pub arguments: u32,
}

// Enum values: winemetal.h's, and airconv_public.h's SM50_BINDING_INDEX.

// WMTStringEncoding
pub const WMTASCII_STRING_ENCODING: u64 = 1;
pub const WMTNEXTSTEP_STRING_ENCODING: u64 = 2;
pub const WMT_JAPANESE_EUC_STRING_ENCODING: u64 = 3;
pub const WMTUTF8_STRING_ENCODING: u64 = 4;
pub const WMTWMTISO_LATIN1_STRING_ENCODING: u64 = 5;
pub const SYMBOL_STRING_ENCODING: u64 = 6;
pub const WMT_NON_LOSSY_ASCII_STRING_ENCODING: u64 = 7;
pub const WMT_SHIFT_JIS_STRING_ENCODING: u64 = 8;
pub const WMTISO_LATIN2_STRING_ENCODING: u64 = 9;
pub const WMT_UNICODE_STRING_ENCODING: u64 = 10;
pub const WMT_WINDOWS_CP1251_STRING_ENCODING: u64 = 11;
pub const WMT_WINDOWS_CP1252_STRING_ENCODING: u64 = 12;
pub const WMT_WINDOWS_CP1253_STRING_ENCODING: u64 = 13;
pub const WMT_WINDOWS_CP1254_STRING_ENCODING: u64 = 14;
pub const WMT_WINDOWS_CP1250_STRING_ENCODING: u64 = 15;
pub const WMTISO2022JP_STRING_ENCODING: u64 = 21;
pub const WMT_MAC_OS_ROMAN_STRING_ENCODING: u64 = 30;
pub const WMTUTF16_STRING_ENCODING: u64 = 10;
pub const WMTUTF16_BIG_ENDIAN_STRING_ENCODING: u64 = 0x9000_0100;
pub const WMTUTF16_LITTLE_ENDIAN_STRING_ENCODING: u64 = 0x9400_0100;
pub const WMTUTF32_STRING_ENCODING: u64 = 0x8C00_0100;
pub const WMTUTF32_BIG_ENDIAN_STRING_ENCODING: u64 = 0x9800_0100;
pub const WMTUTF32_LITTLE_ENDIAN_STRING_ENCODING: u64 = 0x9C00_0100;

// WMTCommandBufferStatus
pub const WMT_COMMAND_BUFFER_STATUS_NOT_ENQUEUED: u64 = 0;
pub const WMT_COMMAND_BUFFER_STATUS_ENQUEUED: u64 = 1;
pub const WMT_COMMAND_BUFFER_STATUS_COMMITTED: u64 = 2;
pub const WMT_COMMAND_BUFFER_STATUS_SCHEDULED: u64 = 3;
pub const WMT_COMMAND_BUFFER_STATUS_COMPLETED: u64 = 4;
pub const WMT_COMMAND_BUFFER_STATUS_ERROR: u64 = 5;

// WMTResourceOptions
pub const WMT_RESOURCE_CPU_CACHE_MODE_DEFAULT_CACHE: u64 = 0;
pub const WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED: u64 = 1;
pub const WMT_RESOURCE_STORAGE_MODE_SHARED: u64 = 0;
pub const WMT_RESOURCE_STORAGE_MODE_MANAGED: u64 = 16;
pub const WMT_RESOURCE_STORAGE_MODE_PRIVATE: u64 = 32;
pub const WMT_RESOURCE_STORAGE_MODE_MEMORYLESS: u64 = 48;
pub const WMT_RESOURCE_HAZARD_TRACKING_MODE_DEFAULT: u64 = 0;
pub const WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED: u64 = 256;
pub const WMT_RESOURCE_HAZARD_TRACKING_MODE_TRACKED: u64 = 512;
pub const WMT_RESOURCE_OPTION_CPU_CACHE_MODE_DEFAULT: u64 = 0;
pub const WMT_RESOURCE_OPTION_CPU_CACHE_MODE_WRITE_COMBINED: u64 = 1;

// WMTSamplerBorderColor
pub const WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK: u8 = 0;
pub const WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK: u8 = 1;
pub const WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE: u8 = 2;

// WMTSamplerAddressMode
pub const WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE: u8 = 0;
pub const WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE: u8 = 1;
pub const WMT_SAMPLER_ADDRESS_MODE_REPEAT: u8 = 2;
pub const WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT: u8 = 3;
pub const WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_ZERO: u8 = 4;
pub const WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR: u8 = 5;

// WMTSamplerMipFilter
pub const WMT_SAMPLER_MIP_FILTER_NOT_MIPMAPPED: u8 = 0;
pub const WMT_SAMPLER_MIP_FILTER_NEAREST: u8 = 1;
pub const WMT_SAMPLER_MIP_FILTER_LINEAR: u8 = 2;

// WMTSamplerMinMagFilter
pub const WMT_SAMPLER_MIN_MAG_FILTER_NEAREST: u8 = 0;
pub const WMT_SAMPLER_MIN_MAG_FILTER_LINEAR: u8 = 1;

// WMTCompareFunction
pub const WMT_COMPARE_FUNCTION_NEVER: u8 = 0;
pub const WMT_COMPARE_FUNCTION_LESS: u8 = 1;
pub const WMT_COMPARE_FUNCTION_EQUAL: u8 = 2;
pub const WMT_COMPARE_FUNCTION_LESS_EQUAL: u8 = 3;
pub const WMT_COMPARE_FUNCTION_GREATER: u8 = 4;
pub const WMT_COMPARE_FUNCTION_NOT_EQUAL: u8 = 5;
pub const WMT_COMPARE_FUNCTION_GREATER_EQUAL: u8 = 6;
pub const WMT_COMPARE_FUNCTION_ALWAYS: u8 = 7;

// WMTStencilOperation
pub const WMT_STENCIL_OPERATION_KEEP: u8 = 0;
pub const WMT_STENCIL_OPERATION_ZERO: u8 = 1;
pub const WMT_STENCIL_OPERATION_REPLACE: u8 = 2;
pub const WMT_STENCIL_OPERATION_INCREMENT_CLAMP: u8 = 3;
pub const WMT_STENCIL_OPERATION_DECREMENT_CLAMP: u8 = 4;
pub const WMT_STENCIL_OPERATION_INVERT: u8 = 5;
pub const WMT_STENCIL_OPERATION_INCREMENT_WRAP: u8 = 6;
pub const WMT_STENCIL_OPERATION_DECREMENT_WRAP: u8 = 7;

// WMTPixelFormat
pub const WMT_PIXEL_FORMAT_INVALID: u32 = 0;
pub const WMT_PIXEL_FORMAT_A8_UNORM: u32 = 1;
pub const WMT_PIXEL_FORMAT_R8_UNORM: u32 = 10;
pub const WMT_PIXEL_FORMAT_R8_UNORM_S_RGB: u32 = 11;
pub const WMT_PIXEL_FORMAT_R8_SNORM: u32 = 12;
pub const WMT_PIXEL_FORMAT_R8_UINT: u32 = 13;
pub const WMT_PIXEL_FORMAT_R8_SINT: u32 = 14;
pub const WMT_PIXEL_FORMAT_R16_UNORM: u32 = 20;
pub const WMT_PIXEL_FORMAT_R16_SNORM: u32 = 22;
pub const WMT_PIXEL_FORMAT_R16_UINT: u32 = 23;
pub const WMT_PIXEL_FORMAT_R16_SINT: u32 = 24;
pub const WMT_PIXEL_FORMAT_R16_FLOAT: u32 = 25;
pub const WMT_PIXEL_FORMAT_RG8_UNORM: u32 = 30;
pub const WMT_PIXEL_FORMAT_RG8_UNORM_S_RGB: u32 = 31;
pub const WMT_PIXEL_FORMAT_RG8_SNORM: u32 = 32;
pub const WMT_PIXEL_FORMAT_RG8_UINT: u32 = 33;
pub const WMT_PIXEL_FORMAT_RG8_SINT: u32 = 34;
pub const WMT_PIXEL_FORMAT_B5G6R5_UNORM: u32 = 40;
pub const WMT_PIXEL_FORMAT_A1BGR5_UNORM: u32 = 41;
pub const WMT_PIXEL_FORMAT_ABGR4_UNORM: u32 = 42;
pub const WMT_PIXEL_FORMAT_BGR5A1_UNORM: u32 = 43;
pub const WMT_PIXEL_FORMAT_R32_UINT: u32 = 53;
pub const WMT_PIXEL_FORMAT_R32_SINT: u32 = 54;
pub const WMT_PIXEL_FORMAT_R32_FLOAT: u32 = 55;
pub const WMT_PIXEL_FORMAT_RG16_UNORM: u32 = 60;
pub const WMT_PIXEL_FORMAT_RG16_SNORM: u32 = 62;
pub const WMT_PIXEL_FORMAT_RG16_UINT: u32 = 63;
pub const WMT_PIXEL_FORMAT_RG16_SINT: u32 = 64;
pub const WMT_PIXEL_FORMAT_RG16_FLOAT: u32 = 65;
pub const WMT_PIXEL_FORMAT_RGBA8_UNORM: u32 = 70;
pub const WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB: u32 = 71;
pub const WMT_PIXEL_FORMAT_RGBA8_SNORM: u32 = 72;
pub const WMT_PIXEL_FORMAT_RGBA8_UINT: u32 = 73;
pub const WMT_PIXEL_FORMAT_RGBA8_SINT: u32 = 74;
pub const WMT_PIXEL_FORMAT_BGRA8_UNORM: u32 = 80;
pub const WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB: u32 = 81;
pub const WMT_PIXEL_FORMAT_RGB10A2_UNORM: u32 = 90;
pub const WMT_PIXEL_FORMAT_RGB10A2_UINT: u32 = 91;
pub const WMT_PIXEL_FORMAT_RG11B10_FLOAT: u32 = 92;
pub const WMT_PIXEL_FORMAT_RGB9E5_FLOAT: u32 = 93;
pub const WMT_PIXEL_FORMAT_BGR10A2_UNORM: u32 = 94;
pub const WMT_PIXEL_FORMAT_BGR10_XR: u32 = 554;
pub const WMT_PIXEL_FORMAT_BGR10_XR_S_RGB: u32 = 555;
pub const WMT_PIXEL_FORMAT_RG32_UINT: u32 = 103;
pub const WMT_PIXEL_FORMAT_RG32_SINT: u32 = 104;
pub const WMT_PIXEL_FORMAT_RG32_FLOAT: u32 = 105;
pub const WMT_PIXEL_FORMAT_RGBA16_UNORM: u32 = 110;
pub const WMT_PIXEL_FORMAT_RGBA16_SNORM: u32 = 112;
pub const WMT_PIXEL_FORMAT_RGBA16_UINT: u32 = 113;
pub const WMT_PIXEL_FORMAT_RGBA16_SINT: u32 = 114;
pub const WMT_PIXEL_FORMAT_RGBA16_FLOAT: u32 = 115;
pub const WMT_PIXEL_FORMAT_BGRA10_XR: u32 = 552;
pub const WMT_PIXEL_FORMAT_BGRA10_XR_S_RGB: u32 = 553;
pub const WMT_PIXEL_FORMAT_RGBA32_UINT: u32 = 123;
pub const WMT_PIXEL_FORMAT_RGBA32_SINT: u32 = 124;
pub const WMT_PIXEL_FORMAT_RGBA32_FLOAT: u32 = 125;
pub const WMT_PIXEL_FORMAT_BC1_RGBA: u32 = 130;
pub const WMT_PIXEL_FORMAT_BC1_RGBA_S_RGB: u32 = 131;
pub const WMT_PIXEL_FORMAT_BC2_RGBA: u32 = 132;
pub const WMT_PIXEL_FORMAT_BC2_RGBA_S_RGB: u32 = 133;
pub const WMT_PIXEL_FORMAT_BC3_RGBA: u32 = 134;
pub const WMT_PIXEL_FORMAT_BC3_RGBA_S_RGB: u32 = 135;
pub const WMT_PIXEL_FORMAT_BC4_R_UNORM: u32 = 140;
pub const WMT_PIXEL_FORMAT_BC4_R_SNORM: u32 = 141;
pub const WMT_PIXEL_FORMAT_BC5_RG_UNORM: u32 = 142;
pub const WMT_PIXEL_FORMAT_BC5_RG_SNORM: u32 = 143;
pub const WMT_PIXEL_FORMAT_BC6H_RGB_FLOAT: u32 = 150;
pub const WMT_PIXEL_FORMAT_BC6H_RGB_UFLOAT: u32 = 151;
pub const WMT_PIXEL_FORMAT_BC7_RGBA_UNORM: u32 = 152;
pub const WMT_PIXEL_FORMAT_BC7_RGBA_UNORM_S_RGB: u32 = 153;
pub const WMT_PIXEL_FORMAT_PVRTC_RGB_2BPP: u32 = 160;
pub const WMT_PIXEL_FORMAT_PVRTC_RGB_2BPP_S_RGB: u32 = 161;
pub const WMT_PIXEL_FORMAT_PVRTC_RGB_4BPP: u32 = 162;
pub const WMT_PIXEL_FORMAT_PVRTC_RGB_4BPP_S_RGB: u32 = 163;
pub const WMT_PIXEL_FORMAT_PVRTC_RGBA_2BPP: u32 = 164;
pub const WMT_PIXEL_FORMAT_PVRTC_RGBA_2BPP_S_RGB: u32 = 165;
pub const WMT_PIXEL_FORMAT_PVRTC_RGBA_4BPP: u32 = 166;
pub const WMT_PIXEL_FORMAT_PVRTC_RGBA_4BPP_S_RGB: u32 = 167;
pub const WMT_PIXEL_FORMAT_EAC_R11_UNORM: u32 = 170;
pub const WMT_PIXEL_FORMAT_EAC_R11_SNORM: u32 = 172;
pub const WMT_PIXEL_FORMAT_EAC_RG11_UNORM: u32 = 174;
pub const WMT_PIXEL_FORMAT_EAC_RG11_SNORM: u32 = 176;
pub const WMT_PIXEL_FORMAT_EAC_RGBA8: u32 = 178;
pub const WMT_PIXEL_FORMAT_EAC_RGBA8_S_RGB: u32 = 179;
pub const WMT_PIXEL_FORMAT_ETC2_RGB8: u32 = 180;
pub const WMT_PIXEL_FORMAT_ETC2_RGB8_S_RGB: u32 = 181;
pub const WMT_PIXEL_FORMAT_ETC2_RGB8A1: u32 = 182;
pub const WMT_PIXEL_FORMAT_ETC2_RGB8A1_S_RGB: u32 = 183;
pub const WMT_PIXEL_FORMAT_ASTC_4X4_S_RGB: u32 = 186;
pub const WMT_PIXEL_FORMAT_ASTC_5X4_S_RGB: u32 = 187;
pub const WMT_PIXEL_FORMAT_ASTC_5X5_S_RGB: u32 = 188;
pub const WMT_PIXEL_FORMAT_ASTC_6X5_S_RGB: u32 = 189;
pub const WMT_PIXEL_FORMAT_ASTC_6X6_S_RGB: u32 = 190;
pub const WMT_PIXEL_FORMAT_ASTC_8X5_S_RGB: u32 = 192;
pub const WMT_PIXEL_FORMAT_ASTC_8X6_S_RGB: u32 = 193;
pub const WMT_PIXEL_FORMAT_ASTC_8X8_S_RGB: u32 = 194;
pub const WMT_PIXEL_FORMAT_ASTC_10X5_S_RGB: u32 = 195;
pub const WMT_PIXEL_FORMAT_ASTC_10X6_S_RGB: u32 = 196;
pub const WMT_PIXEL_FORMAT_ASTC_10X8_S_RGB: u32 = 197;
pub const WMT_PIXEL_FORMAT_ASTC_10X10_S_RGB: u32 = 198;
pub const WMT_PIXEL_FORMAT_ASTC_12X10_S_RGB: u32 = 199;
pub const WMT_PIXEL_FORMAT_ASTC_12X12_S_RGB: u32 = 200;
pub const WMT_PIXEL_FORMAT_ASTC_4X4_LDR: u32 = 204;
pub const WMT_PIXEL_FORMAT_ASTC_5X4_LDR: u32 = 205;
pub const WMT_PIXEL_FORMAT_ASTC_5X5_LDR: u32 = 206;
pub const WMT_PIXEL_FORMAT_ASTC_6X5_LDR: u32 = 207;
pub const WMT_PIXEL_FORMAT_ASTC_6X6_LDR: u32 = 208;
pub const WMT_PIXEL_FORMAT_ASTC_8X5_LDR: u32 = 210;
pub const WMT_PIXEL_FORMAT_ASTC_8X6_LDR: u32 = 211;
pub const WMT_PIXEL_FORMAT_ASTC_8X8_LDR: u32 = 212;
pub const WMT_PIXEL_FORMAT_ASTC_10X5_LDR: u32 = 213;
pub const WMT_PIXEL_FORMAT_ASTC_10X6_LDR: u32 = 214;
pub const WMT_PIXEL_FORMAT_ASTC_10X8_LDR: u32 = 215;
pub const WMT_PIXEL_FORMAT_ASTC_10X10_LDR: u32 = 216;
pub const WMT_PIXEL_FORMAT_ASTC_12X10_LDR: u32 = 217;
pub const WMT_PIXEL_FORMAT_ASTC_12X12_LDR: u32 = 218;
pub const WMT_PIXEL_FORMAT_ASTC_4X4_HDR: u32 = 222;
pub const WMT_PIXEL_FORMAT_ASTC_5X4_HDR: u32 = 223;
pub const WMT_PIXEL_FORMAT_ASTC_5X5_HDR: u32 = 224;
pub const WMT_PIXEL_FORMAT_ASTC_6X5_HDR: u32 = 225;
pub const WMT_PIXEL_FORMAT_ASTC_6X6_HDR: u32 = 226;
pub const WMT_PIXEL_FORMAT_ASTC_8X5_HDR: u32 = 228;
pub const WMT_PIXEL_FORMAT_ASTC_8X6_HDR: u32 = 229;
pub const WMT_PIXEL_FORMAT_ASTC_8X8_HDR: u32 = 230;
pub const WMT_PIXEL_FORMAT_ASTC_10X5_HDR: u32 = 231;
pub const WMT_PIXEL_FORMAT_ASTC_10X6_HDR: u32 = 232;
pub const WMT_PIXEL_FORMAT_ASTC_10X8_HDR: u32 = 233;
pub const WMT_PIXEL_FORMAT_ASTC_10X10_HDR: u32 = 234;
pub const WMT_PIXEL_FORMAT_ASTC_12X10_HDR: u32 = 235;
pub const WMT_PIXEL_FORMAT_ASTC_12X12_HDR: u32 = 236;
pub const WMT_PIXEL_FORMAT_GBGR422: u32 = 240;
pub const WMT_PIXEL_FORMAT_BGRG422: u32 = 241;
pub const WMT_PIXEL_FORMAT_DEPTH16_UNORM: u32 = 250;
pub const WMT_PIXEL_FORMAT_DEPTH32_FLOAT: u32 = 252;
pub const WMT_PIXEL_FORMAT_STENCIL8: u32 = 253;
pub const WMT_PIXEL_FORMAT_DEPTH24_UNORM_STENCIL8: u32 = 255;
pub const WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8: u32 = 260;
pub const WMT_PIXEL_FORMAT_X32_STENCIL8: u32 = 261;
pub const WMT_PIXEL_FORMAT_X24_STENCIL8: u32 = 262;
pub const WMT_PIXEL_FORMAT_CUSTOM_SWIZZLE: u32 = 0xFF_F000;
pub const WMT_PIXEL_FORMAT_ALPHA_IS_ONE: u32 = 8192;
pub const WMT_PIXEL_FORMAT_R001_SWIZZLE: u32 = 0xD_2000;
pub const WMT_PIXEL_FORMAT0R01_SWIZZLE: u32 = 0x95_2000;
pub const WMT_PIXEL_FORMAT_GBAR_SWIZZLE: u32 = 0x24_B000;
pub const WMT_PIXEL_FORMAT_RGB1_SWIZZLE: u32 = 8192;
pub const WMT_PIXEL_FORMAT_BGRX8_UNORM: u32 = 8272;
pub const WMT_PIXEL_FORMAT_BGRX8_UNORM_S_RGB: u32 = 8273;
pub const WMT_PIXEL_FORMAT_R32X8X32: u32 = 0xD_2104;
pub const WMT_PIXEL_FORMAT_X32G8X32: u32 = 0x95_2105;
pub const WMT_PIXEL_FORMAT_BGRA4_UNORM: u32 = 0x24_B02A;

// WMTTextureType
pub const WMT_TEXTURE_TYPE1D: u32 = 0;
pub const WMT_TEXTURE_TYPE1D_ARRAY: u32 = 1;
pub const WMT_TEXTURE_TYPE2D: u32 = 2;
pub const WMT_TEXTURE_TYPE2D_ARRAY: u32 = 3;
pub const WMT_TEXTURE_TYPE2D_MULTISAMPLE: u32 = 4;
pub const WMT_TEXTURE_TYPE_CUBE: u32 = 5;
pub const WMT_TEXTURE_TYPE_CUBE_ARRAY: u32 = 6;
pub const WMT_TEXTURE_TYPE3D: u32 = 7;
pub const WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY: u32 = 8;
pub const WMT_TEXTURE_TYPE_TEXTURE_BUFFER: u32 = 9;

// WMTTextureUsage
pub const WMT_TEXTURE_USAGE_UNKNOWN: u32 = 0;
pub const WMT_TEXTURE_USAGE_SHADER_READ: u32 = 1;
pub const WMT_TEXTURE_USAGE_SHADER_WRITE: u32 = 2;
pub const WMT_TEXTURE_USAGE_RENDER_TARGET: u32 = 4;
pub const WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW: u32 = 16;
pub const WMT_TEXTURE_USAGE_SHADER_ATOMIC: u32 = 32;

// WMTTextureSwizzle
pub const WMT_TEXTURE_SWIZZLE_ZERO: u8 = 0;
pub const WMT_TEXTURE_SWIZZLE_ONE: u8 = 1;
pub const WMT_TEXTURE_SWIZZLE_RED: u8 = 2;
pub const WMT_TEXTURE_SWIZZLE_GREEN: u8 = 3;
pub const WMT_TEXTURE_SWIZZLE_BLUE: u8 = 4;
pub const WMT_TEXTURE_SWIZZLE_ALPHA: u8 = 5;

// WMTTextureFlag
pub const WMT_TEXTURE_FLAG_LOSSLESS: u32 = 1;

// WMTAttributeFormat
pub const WMT_ATTRIBUTE_FORMAT_INVALID: u32 = 0;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR2: u32 = 1;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR3: u32 = 2;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR4: u32 = 3;
pub const WMT_ATTRIBUTE_FORMAT_CHAR2: u32 = 4;
pub const WMT_ATTRIBUTE_FORMAT_CHAR3: u32 = 5;
pub const WMT_ATTRIBUTE_FORMAT_CHAR4: u32 = 6;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR2_NORMALIZED: u32 = 7;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR3_NORMALIZED: u32 = 8;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR4_NORMALIZED: u32 = 9;
pub const WMT_ATTRIBUTE_FORMAT_CHAR2_NORMALIZED: u32 = 10;
pub const WMT_ATTRIBUTE_FORMAT_CHAR3_NORMALIZED: u32 = 11;
pub const WMT_ATTRIBUTE_FORMAT_CHAR4_NORMALIZED: u32 = 12;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT2: u32 = 13;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT3: u32 = 14;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT4: u32 = 15;
pub const WMT_ATTRIBUTE_FORMAT_SHORT2: u32 = 16;
pub const WMT_ATTRIBUTE_FORMAT_SHORT3: u32 = 17;
pub const WMT_ATTRIBUTE_FORMAT_SHORT4: u32 = 18;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT2_NORMALIZED: u32 = 19;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT3_NORMALIZED: u32 = 20;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT4_NORMALIZED: u32 = 21;
pub const WMT_ATTRIBUTE_FORMAT_SHORT2_NORMALIZED: u32 = 22;
pub const WMT_ATTRIBUTE_FORMAT_SHORT3_NORMALIZED: u32 = 23;
pub const WMT_ATTRIBUTE_FORMAT_SHORT4_NORMALIZED: u32 = 24;
pub const WMT_ATTRIBUTE_FORMAT_HALF2: u32 = 25;
pub const WMT_ATTRIBUTE_FORMAT_HALF3: u32 = 26;
pub const WMT_ATTRIBUTE_FORMAT_HALF4: u32 = 27;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT: u32 = 28;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT2: u32 = 29;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT3: u32 = 30;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT4: u32 = 31;
pub const WMT_ATTRIBUTE_FORMAT_INT: u32 = 32;
pub const WMT_ATTRIBUTE_FORMAT_INT2: u32 = 33;
pub const WMT_ATTRIBUTE_FORMAT_INT3: u32 = 34;
pub const WMT_ATTRIBUTE_FORMAT_INT4: u32 = 35;
pub const WMT_ATTRIBUTE_FORMAT_U_INT: u32 = 36;
pub const WMT_ATTRIBUTE_FORMAT_U_INT2: u32 = 37;
pub const WMT_ATTRIBUTE_FORMAT_U_INT3: u32 = 38;
pub const WMT_ATTRIBUTE_FORMAT_U_INT4: u32 = 39;
pub const WMT_ATTRIBUTE_FORMAT_INT1010102_NORMALIZED: u32 = 40;
pub const WMT_ATTRIBUTE_FORMAT_U_INT1010102_NORMALIZED: u32 = 41;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR4_NORMALIZED_BGRA: u32 = 42;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR: u32 = 45;
pub const WMT_ATTRIBUTE_FORMAT_CHAR: u32 = 46;
pub const WMT_ATTRIBUTE_FORMAT_U_CHAR_NORMALIZED: u32 = 47;
pub const WMT_ATTRIBUTE_FORMAT_CHAR_NORMALIZED: u32 = 48;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT: u32 = 49;
pub const WMT_ATTRIBUTE_FORMAT_SHORT: u32 = 50;
pub const WMT_ATTRIBUTE_FORMAT_U_SHORT_NORMALIZED: u32 = 51;
pub const WMT_ATTRIBUTE_FORMAT_SHORT_NORMALIZED: u32 = 52;
pub const WMT_ATTRIBUTE_FORMAT_HALF: u32 = 53;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT_RG11B10: u32 = 54;
pub const WMT_ATTRIBUTE_FORMAT_FLOAT_RGB9E5: u32 = 55;

// WMTLoadAction
pub const WMT_LOAD_ACTION_DONT_CARE: u32 = 0;
pub const WMT_LOAD_ACTION_LOAD: u32 = 1;
pub const WMT_LOAD_ACTION_CLEAR: u32 = 2;

// WMTStoreAction
pub const WMT_STORE_ACTION_DONT_CARE: u32 = 0;
pub const WMT_STORE_ACTION_STORE: u32 = 1;
pub const WMT_STORE_ACTION_MULTISAMPLE_RESOLVE: u32 = 2;
pub const WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE: u32 = 3;
pub const WMT_STORE_ACTION_UNKNOWN: u32 = 4;
pub const WMT_STORE_ACTION_CUSTOM_SAMPLE_DEPTH_STORE: u32 = 5;

// WMTTessellationPartitionMode
pub const WMT_TESSELLATION_PARTITION_MODE_POW2: u8 = 0;
pub const WMT_TESSELLATION_PARTITION_MODE_INTEGER: u8 = 1;
pub const WMT_TESSELLATION_PARTITION_MODE_FRACTIONAL_ODD: u8 = 2;
pub const WMT_TESSELLATION_PARTITION_MODE_FRACTIONAL_EVEN: u8 = 3;

// WMTTessellationFactorStepFunction
pub const WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT: u8 = 0;
pub const WMT_TESSELLATION_FACTOR_STEP_FUNCTION_PER_PATCH: u8 = 1;
pub const WMT_TESSELLATION_FACTOR_STEP_FUNCTION_PER_INSTANCE: u8 = 2;
pub const WMT_TESSELLATION_FACTOR_STEP_FUNCTION_PER_PATCH_AND_PER_INSTANCE: u8 = 3;

// WMTWinding
pub const WMT_WINDING_CLOCKWISE: u8 = 0;
pub const WMT_WINDING_COUNTER_CLOCKWISE: u8 = 1;

// WMTBlendOperation
pub const WMT_BLEND_OPERATION_ADD: u8 = 0;
pub const WMT_BLEND_OPERATION_SUBTRACT: u8 = 1;
pub const WMT_BLEND_OPERATION_REVERSE_SUBTRACT: u8 = 2;
pub const WMT_BLEND_OPERATION_MIN: u8 = 3;
pub const WMT_BLEND_OPERATION_MAX: u8 = 4;

// WMTLogicOperation
pub const WMT_LOGIC_OPERATION_CLEAR: u8 = 0;
pub const WMT_LOGIC_OPERATION_SET: u8 = 1;
pub const WMT_LOGIC_OPERATION_COPY: u8 = 2;
pub const WMT_LOGIC_OPERATION_COPY_INVERTED: u8 = 3;
pub const WMT_LOGIC_OPERATION_NO_OP: u8 = 4;
pub const WMT_LOGIC_OPERATION_INVERT: u8 = 5;
pub const WMT_LOGIC_OPERATION_AND: u8 = 6;
pub const WMT_LOGIC_OPERATION_NAND: u8 = 7;
pub const WMT_LOGIC_OPERATION_OR: u8 = 8;
pub const WMT_LOGIC_OPERATION_NOR: u8 = 9;
pub const WMT_LOGIC_OPERATION_XOR: u8 = 10;
pub const WMT_LOGIC_OPERATION_EQUIV: u8 = 11;
pub const WMT_LOGIC_OPERATION_AND_REVERSE: u8 = 12;
pub const WMT_LOGIC_OPERATION_AND_INVERTED: u8 = 13;
pub const WMT_LOGIC_OPERATION_OR_REVERSE: u8 = 14;
pub const WMT_LOGIC_OPERATION_OR_INVERTED: u8 = 15;

// WMTBlendFactor
pub const WMT_BLEND_FACTOR_ZERO: u8 = 0;
pub const WMT_BLEND_FACTOR_ONE: u8 = 1;
pub const WMT_BLEND_FACTOR_SOURCE_COLOR: u8 = 2;
pub const WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR: u8 = 3;
pub const WMT_BLEND_FACTOR_SOURCE_ALPHA: u8 = 4;
pub const WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA: u8 = 5;
pub const WMT_BLEND_FACTOR_DESTINATION_COLOR: u8 = 6;
pub const WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR: u8 = 7;
pub const WMT_BLEND_FACTOR_DESTINATION_ALPHA: u8 = 8;
pub const WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA: u8 = 9;
pub const WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED: u8 = 10;
pub const WMT_BLEND_FACTOR_BLEND_COLOR: u8 = 11;
pub const WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR: u8 = 12;
pub const WMT_BLEND_FACTOR_BLEND_ALPHA: u8 = 13;
pub const WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA: u8 = 14;
pub const WMT_BLEND_FACTOR_SOURCE1_COLOR: u8 = 15;
pub const WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_COLOR: u8 = 16;
pub const WMT_BLEND_FACTOR_SOURCE1_ALPHA: u8 = 17;
pub const WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_ALPHA: u8 = 18;

// WMTColorWriteMask
pub const WMT_COLOR_WRITE_MASK_NONE: u8 = 0;
pub const WMT_COLOR_WRITE_MASK_RED: u8 = 8;
pub const WMT_COLOR_WRITE_MASK_GREEN: u8 = 4;
pub const WMT_COLOR_WRITE_MASK_BLUE: u8 = 2;
pub const WMT_COLOR_WRITE_MASK_ALPHA: u8 = 1;
pub const WMT_COLOR_WRITE_MASK_ALL: u8 = 15;

// WMTPrimitiveTopologyClass
pub const WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED: u32 = 0;
pub const WMT_PRIMITIVE_TOPOLOGY_CLASS_POINT: u32 = 1;
pub const WMT_PRIMITIVE_TOPOLOGY_CLASS_LINE: u32 = 2;
pub const WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE: u32 = 3;

// WMTBlitCommandType
pub const WMT_BLIT_COMMAND_NOP: u16 = 0;
pub const WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER: u16 = 1;
pub const WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE: u16 = 2;
pub const WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER: u16 = 3;
pub const WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE: u16 = 4;
pub const WMT_BLIT_COMMAND_GENERATE_MIPMAPS: u16 = 5;
pub const WMT_BLIT_COMMAND_WAIT_FOR_FENCE: u16 = 6;
pub const WMT_BLIT_COMMAND_UPDATE_FENCE: u16 = 7;
pub const WMT_BLIT_COMMAND_FILL_BUFFER: u16 = 8;
pub const WMT_BLIT_COMMAND_RESOLVE_COUNTERS: u16 = 9;
pub const WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION: u16 = 10;
pub const WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION: u16 = 11;
pub const WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER: u16 = 12;
pub const WMT_BLIT_COMMAND_COPY_TEXTURE: u16 = 13;

// WMTBlitOption
pub const WMT_BLIT_OPTION_NONE: u16 = 0;
pub const WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL: u16 = 1;
pub const WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL: u16 = 2;
pub const WMT_BLIT_OPTION_ROW_LINEAR_PVRTC: u16 = 4;

// WMTComputeCommandType
pub const WMT_COMPUTE_COMMAND_NOP: u16 = 0;
pub const WMT_COMPUTE_COMMAND_DISPATCH: u16 = 1;
pub const WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT: u16 = 2;
pub const WMT_COMPUTE_COMMAND_SET_PSO: u16 = 3;
pub const WMT_COMPUTE_COMMAND_SET_BUFFER: u16 = 4;
pub const WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET: u16 = 5;
pub const WMT_COMPUTE_COMMAND_USE_RESOURCE: u16 = 6;
pub const WMT_COMPUTE_COMMAND_SET_BYTES: u16 = 7;
pub const WMT_COMPUTE_COMMAND_SET_TEXTURE: u16 = 8;
pub const WMT_COMPUTE_COMMAND_DISPATCH_THREADS: u16 = 9;
pub const WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE: u16 = 10;
pub const WMT_COMPUTE_COMMAND_UPDATE_FENCE: u16 = 11;
pub const WMT_COMPUTE_COMMAND_MEMORY_BARRIER: u16 = 12;
pub const WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER: u16 = 13;

// WMTResourceUsage
pub const WMT_RESOURCE_USAGE_READ: u32 = 1;
pub const WMT_RESOURCE_USAGE_WRITE: u32 = 2;
pub const WMT_RESOURCE_USAGE_SAMPLE: u32 = 4;

// WMTBarrierScope
pub const WMT_BARRIER_SCOPE_BUFFERS: u8 = 1;
pub const WMT_BARRIER_SCOPE_TEXTURES: u8 = 2;
pub const WMT_BARRIER_SCOPE_RENDER_TARGETS: u8 = 4;

// WMTRenderCommandType
pub const WMT_RENDER_COMMAND_NOP: u16 = 0;
pub const WMT_RENDER_COMMAND_USE_RESOURCE: u16 = 1;
pub const WMT_RENDER_COMMAND_SET_VERTEX_BUFFER: u16 = 2;
pub const WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET: u16 = 3;
pub const WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER: u16 = 4;
pub const WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET: u16 = 5;
pub const WMT_RENDER_COMMAND_SET_MESH_BUFFER: u16 = 6;
pub const WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET: u16 = 7;
pub const WMT_RENDER_COMMAND_SET_OBJECT_BUFFER: u16 = 8;
pub const WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET: u16 = 9;
pub const WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE: u16 = 10;
pub const WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES: u16 = 11;
pub const WMT_RENDER_COMMAND_SET_RASTERIZER_STATE: u16 = 12;
pub const WMT_RENDER_COMMAND_SET_VIEWPORTS: u16 = 13;
pub const WMT_RENDER_COMMAND_SET_SCISSOR_RECTS: u16 = 14;
pub const WMT_RENDER_COMMAND_SET_PSO: u16 = 15;
pub const WMT_RENDER_COMMAND_SET_DSSO: u16 = 16;
pub const WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF: u16 = 17;
pub const WMT_RENDER_COMMAND_SET_VISIBILITY_MODE: u16 = 18;
pub const WMT_RENDER_COMMAND_DRAW: u16 = 19;
pub const WMT_RENDER_COMMAND_DRAW_INDEXED: u16 = 20;
pub const WMT_RENDER_COMMAND_DRAW_INDIRECT: u16 = 21;
pub const WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT: u16 = 22;
pub const WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS: u16 = 23;
pub const WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT: u16 = 24;
pub const WMT_RENDER_COMMAND_MEMORY_BARRIER: u16 = 25;
pub const UNUSED0: u16 = 26;
pub const UNUSED1: u16 = 27;
pub const UNUSED2: u16 = 28;
pub const WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW: u16 = 29;
pub const WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED: u16 = 30;
pub const WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT: u16 = 31;
pub const WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT: u16 = 32;
pub const WMT_RENDER_COMMAND_WAIT_FOR_FENCE: u16 = 33;
pub const WMT_RENDER_COMMAND_UPDATE_FENCE: u16 = 34;
pub const WMT_RENDER_COMMAND_SET_VIEWPORT: u16 = 35;
pub const WMT_RENDER_COMMAND_SET_SCISSOR_RECT: u16 = 36;
pub const WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW: u16 = 37;
pub const WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED: u16 = 38;
pub const WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT: u16 = 39;
pub const WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT: u16 = 40;
pub const WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE: u16 = 41;
pub const WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER: u16 = 42;
pub const WMT_RENDER_COMMAND_SET_BLEND_FACTOR: u16 = 43;
pub const WMT_RENDER_COMMAND_SET_STENCIL_REF: u16 = 44;
pub const WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE: u16 = 45;
pub const WMT_RENDER_COMMAND_SET_VERTEX_BYTES: u16 = 46;
pub const WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE: u16 = 47;
pub const WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE: u16 = 48;
pub const WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE: u16 = 49;

// WMTRenderStages
pub const WMT_RENDER_STAGE_VERTEX: u8 = 1;
pub const WMT_RENDER_STAGE_FRAGMENT: u8 = 2;
pub const WMT_RENDER_STAGE_TILE: u8 = 4;
pub const WMT_RENDER_STAGE_OBJECT: u8 = 8;
pub const WMT_RENDER_STAGE_MESH: u8 = 16;
pub const WMT_RENDER_STAGE_PRE_RASTER: u8 = 25;

// WMTTriangleFillMode
pub const WMT_TRIANGLE_FILL_MODE_FILL: u8 = 0;
pub const WMT_TRIANGLE_FILL_MODE_LINES: u8 = 1;

// WMTCullMode
pub const WMT_CULL_MODE_NONE: u8 = 0;
pub const WMT_CULL_MODE_FRONT: u8 = 1;
pub const WMT_CULL_MODE_BACK: u8 = 2;

// WMTDepthClipMode
pub const WMT_DEPTH_CLIP_MODE_CLIP: u8 = 0;
pub const WMT_DEPTH_CLIP_MODE_CLAMP: u8 = 1;

// WMTVisibilityResultMode
pub const WMT_VISIBILITY_RESULT_MODE_DISABLED: u8 = 0;
pub const WMT_VISIBILITY_RESULT_MODE_BOOLEAN: u8 = 1;
pub const WMT_VISIBILITY_RESULT_MODE_COUNTING: u8 = 2;

// WMTIndexType
pub const WMT_INDEX_TYPE_U_INT16: u8 = 0;
pub const WMT_INDEX_TYPE_U_INT32: u8 = 1;

// WMTPrimitiveType
pub const WMT_PRIMITIVE_TYPE_POINT: u8 = 0;
pub const WMT_PRIMITIVE_TYPE_LINE: u8 = 1;
pub const WMT_PRIMITIVE_TYPE_LINE_STRIP: u8 = 2;
pub const WMT_PRIMITIVE_TYPE_TRIANGLE: u8 = 3;
pub const WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP: u8 = 4;

// WMTGPUFamily
pub const WMTGPU_FAMILY_APPLE1: u32 = 1001;
pub const WMTGPU_FAMILY_APPLE2: u32 = 1002;
pub const WMTGPU_FAMILY_APPLE3: u32 = 1003;
pub const WMTGPU_FAMILY_APPLE4: u32 = 1004;
pub const WMTGPU_FAMILY_APPLE5: u32 = 1005;
pub const WMTGPU_FAMILY_APPLE6: u32 = 1006;
pub const WMTGPU_FAMILY_APPLE7: u32 = 1007;
pub const WMTGPU_FAMILY_APPLE8: u32 = 1008;
pub const WMTGPU_FAMILY_APPLE9: u32 = 1009;
pub const WMTGPU_FAMILY_MAC1: u32 = 2001;
pub const WMTGPU_FAMILY_MAC2: u32 = 2002;
pub const WMTGPU_FAMILY_COMMON1: u32 = 3001;
pub const WMTGPU_FAMILY_COMMON2: u32 = 3002;
pub const WMTGPU_FAMILY_COMMON3: u32 = 3003;
pub const WMTGPU_FAMILY_MAC_CATALYST1: u32 = 4001;
pub const WMTGPU_FAMILY_MAC_CATALYST2: u32 = 4002;
pub const WMTGPU_FAMILY_METAL3: u32 = 5001;

// WMTCaptureDestination
pub const WMT_CAPTURE_DESTINATION_DEVELOPER_TOOLS: u8 = 1;
pub const WMT_CAPTURE_DESTINATION_GPU_TRACE_DOCUMENT: u8 = 2;

// WMTColorSpace
pub const WMT_COLOR_SPACE_SRGB: u64 = 0;
pub const WMT_COLOR_SPACE_SRGB_LINEAR: u64 = 1;
pub const WMT_COLOR_SPACE_BT2020: u64 = 2;
pub const WMT_COLOR_SPACE_HDR_PQ: u64 = 4;
pub const WMT_COLOR_SPACE_HDR10: u64 = 4;
pub const WMT_COLOR_SPACE_HDR_SC_RGB: u64 = 5;
pub const WMT_COLOR_SPACE_INVALID: u64 = 0xFFFF_FFFF_FFFF_FFFF;

// WMTDataType
pub const WMT_DATA_TYPE_NONE: u16 = 0;
pub const WMT_DATA_TYPE_STRUCT: u16 = 1;
pub const WMT_DATA_TYPE_ARRAY: u16 = 2;
pub const WMT_DATA_TYPE_FLOAT: u16 = 3;
pub const WMT_DATA_TYPE_FLOAT2: u16 = 4;
pub const WMT_DATA_TYPE_FLOAT3: u16 = 5;
pub const WMT_DATA_TYPE_FLOAT4: u16 = 6;
pub const WMT_DATA_TYPE_FLOAT2X2: u16 = 7;
pub const WMT_DATA_TYPE_FLOAT2X3: u16 = 8;
pub const WMT_DATA_TYPE_FLOAT2X4: u16 = 9;
pub const WMT_DATA_TYPE_FLOAT3X2: u16 = 10;
pub const WMT_DATA_TYPE_FLOAT3X3: u16 = 11;
pub const WMT_DATA_TYPE_FLOAT3X4: u16 = 12;
pub const WMT_DATA_TYPE_FLOAT4X2: u16 = 13;
pub const WMT_DATA_TYPE_FLOAT4X3: u16 = 14;
pub const WMT_DATA_TYPE_FLOAT4X4: u16 = 15;
pub const WMT_DATA_TYPE_HALF: u16 = 16;
pub const WMT_DATA_TYPE_HALF2: u16 = 17;
pub const WMT_DATA_TYPE_HALF3: u16 = 18;
pub const WMT_DATA_TYPE_HALF4: u16 = 19;
pub const WMT_DATA_TYPE_HALF2X2: u16 = 20;
pub const WMT_DATA_TYPE_HALF2X3: u16 = 21;
pub const WMT_DATA_TYPE_HALF2X4: u16 = 22;
pub const WMT_DATA_TYPE_HALF3X2: u16 = 23;
pub const WMT_DATA_TYPE_HALF3X3: u16 = 24;
pub const WMT_DATA_TYPE_HALF3X4: u16 = 25;
pub const WMT_DATA_TYPE_HALF4X2: u16 = 26;
pub const WMT_DATA_TYPE_HALF4X3: u16 = 27;
pub const WMT_DATA_TYPE_HALF4X4: u16 = 28;
pub const WMT_DATA_TYPE_INT: u16 = 29;
pub const WMT_DATA_TYPE_INT2: u16 = 30;
pub const WMT_DATA_TYPE_INT3: u16 = 31;
pub const WMT_DATA_TYPE_INT4: u16 = 32;
pub const WMT_DATA_TYPE_U_INT: u16 = 33;
pub const WMT_DATA_TYPE_U_INT2: u16 = 34;
pub const WMT_DATA_TYPE_U_INT3: u16 = 35;
pub const WMT_DATA_TYPE_U_INT4: u16 = 36;
pub const WMT_DATA_TYPE_SHORT: u16 = 37;
pub const WMT_DATA_TYPE_SHORT2: u16 = 38;
pub const WMT_DATA_TYPE_SHORT3: u16 = 39;
pub const WMT_DATA_TYPE_SHORT4: u16 = 40;
pub const WMT_DATA_TYPE_U_SHORT: u16 = 41;
pub const WMT_DATA_TYPE_U_SHORT2: u16 = 42;
pub const WMT_DATA_TYPE_U_SHORT3: u16 = 43;
pub const WMT_DATA_TYPE_U_SHORT4: u16 = 44;
pub const WMT_DATA_TYPE_CHAR: u16 = 45;
pub const WMT_DATA_TYPE_CHAR2: u16 = 46;
pub const WMT_DATA_TYPE_CHAR3: u16 = 47;
pub const WMT_DATA_TYPE_CHAR4: u16 = 48;
pub const WMT_DATA_TYPE_U_CHAR: u16 = 49;
pub const WMT_DATA_TYPE_U_CHAR2: u16 = 50;
pub const WMT_DATA_TYPE_U_CHAR3: u16 = 51;
pub const WMT_DATA_TYPE_U_CHAR4: u16 = 52;
pub const WMT_DATA_TYPE_BOOL: u16 = 53;
pub const WMT_DATA_TYPE_BOOL2: u16 = 54;
pub const WMT_DATA_TYPE_BOOL3: u16 = 55;
pub const WMT_DATA_TYPE_BOOL4: u16 = 56;

// WMTMetalVersion
pub const WMT_METAL310: u32 = 310;
pub const WMT_METAL320: u32 = 320;
pub const WMT_METAL_VERSION_MAX: u32 = 320;

// WMTCommandBufferProperty
pub const WMT_COMMAND_BUFFER_PROPERTY_KERNEL_START_TIME: u32 = 0;
pub const WMT_COMMAND_BUFFER_PROPERTY_KERNEL_END_TIME: u32 = 1;
pub const WMT_COMMAND_BUFFER_PROPERTY_GPU_START_TIME: u32 = 2;
pub const WMT_COMMAND_BUFFER_PROPERTY_GPU_END_TIME: u32 = 3;

// WMTHeapType
pub const WMT_HEAP_TYPE_AUTOMATIC: u32 = 0;
pub const WMT_HEAP_TYPE_PLACEMENT: u32 = 1;
pub const WMT_HEAP_TYPE_SPARSE: u32 = 2;

// WMTSparsePageSize
pub const WMT_SPARSE_PAGE_SIZE16: u32 = 101;
pub const WMT_SPARSE_PAGE_SIZE64: u32 = 102;
pub const WMT_SPARSE_PAGE_SIZE256: u32 = 103;

// WMTIndirectCommandType
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW: u32 = 1;
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW_INDEXED: u32 = 2;
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW_PATCHES: u32 = 4;
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW_INDEXED_PATCHES: u32 = 8;
pub const WMT_INDIRECT_COMMAND_TYPE_CONCURRENT_DISPATCH: u32 = 32;
pub const WMT_INDIRECT_COMMAND_TYPE_CONCURRENT_DISPATCH_THREADS: u32 = 64;
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW_MESH_THREADGROUPS: u32 = 128;
pub const WMT_INDIRECT_COMMAND_TYPE_DRAW_MESH_THREADS: u32 = 256;

// SM50_BINDING_INDEX
pub const SM50_BINDING_INDEX_CONSTANT_BUFFER: u32 = 0;
pub const SM50_BINDING_INDEX_ARGUMENT_TABLE: u32 = 1;
pub const SM50_BINDING_INDEX_VERTEX_BUFFER: u32 = 2;
pub const SM50_BINDING_INDEX_INDEX_BUFFER: u32 = 3;
pub const SM50_BINDING_INDEX_DRAW_ARGUMENTS: u32 = 4;
pub const SM50_BINDING_INDEX_INDIRECT_ARGUMENTS: u32 = 4;
pub const SM50_BINDING_INDEX_STREAM_OUTPUT0: u32 = 5;
pub const SM50_BINDING_INDEX_CONSTANT_BUFFER2: u32 = 6;
pub const SM50_BINDING_INDEX_ARGUMENT_TABLE2: u32 = 7;
pub const SM50_BINDING_INDEX_ROOT_ARGUMENTS: u32 = 0;
pub const SM50_BINDING_INDEX_STATIC_SAMPLERS: u32 = 1;
