//! The pipeline state stream's subobject types and descriptions.
//!
//! The structures and constants as `d3d12.h` defines them. Every enumeration is its `u32` (the C
//! compilers give a header enumeration of non-negative values that type), a `BOOL` its `i32`, and
//! the fields are the C's in snake case. A structure's layout is the C's on every target, asserted
//! at the end against clang's record layouts (llvm-mingw's sysroot for the four PE targets,
//! `include/native` natively), the stream's `{ type, data }` wrappers among them.

use core::{ffi::c_void, mem::offset_of};

pub use dxmt_util::com::{E_INVALIDARG, E_NOTIMPL, S_OK};

pub const SUBOBJECT_TYPE_ROOT_SIGNATURE: u32 = 0x0;
pub const SUBOBJECT_TYPE_VS: u32 = 0x1;
pub const SUBOBJECT_TYPE_PS: u32 = 0x2;
pub const SUBOBJECT_TYPE_DS: u32 = 0x3;
pub const SUBOBJECT_TYPE_HS: u32 = 0x4;
pub const SUBOBJECT_TYPE_GS: u32 = 0x5;
pub const SUBOBJECT_TYPE_CS: u32 = 0x6;
pub const SUBOBJECT_TYPE_STREAM_OUTPUT: u32 = 0x7;
pub const SUBOBJECT_TYPE_BLEND: u32 = 0x8;
pub const SUBOBJECT_TYPE_SAMPLE_MASK: u32 = 0x9;
pub const SUBOBJECT_TYPE_RASTERIZER: u32 = 0xa;
pub const SUBOBJECT_TYPE_DEPTH_STENCIL: u32 = 0xb;
pub const SUBOBJECT_TYPE_INPUT_LAYOUT: u32 = 0xc;
pub const SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE: u32 = 0xd;
pub const SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY: u32 = 0xe;
pub const SUBOBJECT_TYPE_RENDER_TARGET_FORMATS: u32 = 0xf;
pub const SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT: u32 = 0x10;
pub const SUBOBJECT_TYPE_SAMPLE_DESC: u32 = 0x11;
pub const SUBOBJECT_TYPE_NODE_MASK: u32 = 0x12;
pub const SUBOBJECT_TYPE_CACHED_PSO: u32 = 0x13;
pub const SUBOBJECT_TYPE_FLAGS: u32 = 0x14;
pub const SUBOBJECT_TYPE_DEPTH_STENCIL1: u32 = 0x15;
pub const SUBOBJECT_TYPE_VIEW_INSTANCING: u32 = 0x16;
pub const SUBOBJECT_TYPE_AS: u32 = 0x18;
pub const SUBOBJECT_TYPE_MS: u32 = 0x19;

/// `D3D12_DEPTH_WRITE_MASK_ALL`.
pub const DEPTH_WRITE_MASK_ALL: u32 = 1;
/// `D3D12_COMPARISON_FUNC_LESS`.
pub const COMPARISON_FUNC_LESS: u32 = 2;
/// `D3D12_COMPARISON_FUNC_ALWAYS`.
pub const COMPARISON_FUNC_ALWAYS: u32 = 8;
/// `D3D12_STENCIL_OP_KEEP`.
pub const STENCIL_OP_KEEP: u32 = 1;
/// `D3D12_FILL_MODE_SOLID`.
pub const FILL_MODE_SOLID: u32 = 3;
/// `D3D12_CULL_MODE_BACK`.
pub const CULL_MODE_BACK: u32 = 3;
/// `D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF`.
pub const CONSERVATIVE_RASTERIZATION_MODE_OFF: u32 = 0;
/// `D3D12_COLOR_WRITE_ENABLE_ALL`.
pub const COLOR_WRITE_ENABLE_ALL: u8 = 0xf;
/// `D3D12_DEFAULT_SAMPLE_MASK`.
pub const DEFAULT_SAMPLE_MASK: u32 = 0xffff_ffff;

/// `D3D12_SHADER_BYTECODE`.
#[repr(C)]
pub struct ShaderBytecode {
    pub shader_bytecode: *const c_void,
    pub bytecode_length: usize,
}

/// `D3D12_SO_DECLARATION_ENTRY`.
#[repr(C)]
pub struct SoDeclarationEntry {
    pub stream: u32,
    pub semantic_name: *const u8,
    pub semantic_index: u32,
    pub start_component: u8,
    pub component_count: u8,
    pub output_slot: u8,
}

/// `D3D12_STREAM_OUTPUT_DESC`.
#[repr(C)]
pub struct StreamOutputDesc {
    pub so_declaration: *const SoDeclarationEntry,
    pub num_entries: u32,
    pub buffer_strides: *const u32,
    pub num_strides: u32,
    pub rasterized_stream: u32,
}

/// `D3D12_RENDER_TARGET_BLEND_DESC`.
#[repr(C)]
pub struct RenderTargetBlendDesc {
    pub blend_enable: i32,
    pub logic_op_enable: i32,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub logic_op: u32,
    pub render_target_write_mask: u8,
}

/// `D3D12_BLEND_DESC`.
#[repr(C)]
pub struct BlendDesc {
    pub alpha_to_coverage_enable: i32,
    pub independent_blend_enable: i32,
    pub render_target: [RenderTargetBlendDesc; 8],
}

/// `D3D12_RASTERIZER_DESC`.
#[repr(C)]
pub struct RasterizerDesc {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
    pub forced_sample_count: u32,
    pub conservative_raster: u32,
}

/// `D3D12_DEPTH_STENCILOP_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DepthStencilOpDesc {
    pub stencil_fail_op: u32,
    pub stencil_depth_fail_op: u32,
    pub stencil_pass_op: u32,
    pub stencil_func: u32,
}

/// `D3D12_DEPTH_STENCIL_DESC`.
#[repr(C)]
pub struct DepthStencilDesc {
    pub depth_enable: i32,
    pub depth_write_mask: u32,
    pub depth_func: u32,
    pub stencil_enable: i32,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: DepthStencilOpDesc,
    pub back_face: DepthStencilOpDesc,
}

/// `D3D12_DEPTH_STENCIL_DESC1`.
#[repr(C)]
pub struct DepthStencilDesc1 {
    pub depth_enable: i32,
    pub depth_write_mask: u32,
    pub depth_func: u32,
    pub stencil_enable: i32,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: DepthStencilOpDesc,
    pub back_face: DepthStencilOpDesc,
    pub depth_bounds_test_enable: i32,
}

/// `D3D12_INPUT_ELEMENT_DESC`.
#[repr(C)]
pub struct InputElementDesc {
    pub semantic_name: *const u8,
    pub semantic_index: u32,
    pub format: u32,
    pub input_slot: u32,
    pub aligned_byte_offset: u32,
    pub input_slot_class: u32,
    pub instance_data_step_rate: u32,
}

/// `D3D12_INPUT_LAYOUT_DESC`.
#[repr(C)]
pub struct InputLayoutDesc {
    pub input_element_descs: *const InputElementDesc,
    pub num_elements: u32,
}

/// `DXGI_SAMPLE_DESC`.
#[repr(C)]
pub struct SampleDesc {
    pub count: u32,
    pub quality: u32,
}

/// `D3D12_CACHED_PIPELINE_STATE`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CachedPipelineState {
    pub cached_blob: *const c_void,
    pub cached_blob_size_in_bytes: usize,
}

/// `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
#[repr(C)]
pub struct GraphicsPipelineStateDesc {
    pub root_signature: *mut c_void,
    pub vs: ShaderBytecode,
    pub ps: ShaderBytecode,
    pub ds: ShaderBytecode,
    pub hs: ShaderBytecode,
    pub gs: ShaderBytecode,
    pub stream_output: StreamOutputDesc,
    pub blend_state: BlendDesc,
    pub sample_mask: u32,
    pub rasterizer_state: RasterizerDesc,
    pub depth_stencil_state: DepthStencilDesc,
    pub input_layout: InputLayoutDesc,
    pub ib_strip_cut_value: u32,
    pub primitive_topology_type: u32,
    pub num_render_targets: u32,
    pub rtv_formats: [u32; 8],
    pub dsv_format: u32,
    pub sample_desc: SampleDesc,
    pub node_mask: u32,
    pub cached_pso: CachedPipelineState,
    pub flags: u32,
}

/// `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
#[repr(C)]
pub struct ComputePipelineStateDesc {
    pub root_signature: *mut c_void,
    pub cs: ShaderBytecode,
    pub node_mask: u32,
    pub cached_pso: CachedPipelineState,
    pub flags: u32,
}

/// `D3D12_PIPELINE_STATE_STREAM_DESC`.
#[repr(C)]
pub struct PipelineStateStreamDesc {
    pub size_in_bytes: usize,
    pub pipeline_state_subobject_stream: *mut c_void,
}

/// `D3D12_RT_FORMAT_ARRAY`.
#[repr(C)]
pub struct RtFormatArray {
    pub rt_formats: [u32; 8],
    pub num_render_targets: u32,
}

/// `D3D12_VIEW_INSTANCE_LOCATION`.
#[repr(C)]
pub struct ViewInstanceLocation {
    pub viewport_array_index: u32,
    pub render_target_array_index: u32,
}

/// `D3D12_VIEW_INSTANCING_DESC`.
#[repr(C)]
pub struct ViewInstancingDesc {
    pub view_instance_count: u32,
    pub view_instance_locations: *const ViewInstanceLocation,
    pub flags: u32,
}

/// A subobject as a stream lays it out: its type, then the data at the data's own alignment.
///
/// The C++ names this shape with a local `struct { type; data_type data; }`, and d3dx12's
/// `CD3DX12_PIPELINE_STATE_STREAM_SUBOBJECT` writes the same one, aligned to a pointer.
#[repr(C)]
pub struct Subobject<T> {
    pub ty: u32,
    pub data: T,
}

// The layouts of clang's `-fdump-record-layouts` over the headers named above: every mirrored
// structure's size, alignment and field offsets. The 32-bit block is i686 PE's; the 64-bit one is
// the x86-64, ARM64 and ARM64EC PE targets' and darwin's, whose dumps are the same.
// A stream's wrapper is `struct { D3D12_PIPELINE_STATE_SUBOBJECT_TYPE type; T data; }` for each T the
// parser reads; the index buffer strip cut value, the topology type, `DXGI_FORMAT` and the state flags lay
// out as the `UINT` one.
#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<BlendDesc>() == 328 && align_of::<BlendDesc>() == 4);
    assert!(offset_of!(BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(BlendDesc, render_target) == 8);
    assert!(size_of::<CachedPipelineState>() == 8 && align_of::<CachedPipelineState>() == 4);
    assert!(offset_of!(CachedPipelineState, cached_blob) == 0);
    assert!(offset_of!(CachedPipelineState, cached_blob_size_in_bytes) == 4);
    assert!(
        size_of::<ComputePipelineStateDesc>() == 28 && align_of::<ComputePipelineStateDesc>() == 4
    );
    assert!(offset_of!(ComputePipelineStateDesc, root_signature) == 0);
    assert!(offset_of!(ComputePipelineStateDesc, cs) == 4);
    assert!(offset_of!(ComputePipelineStateDesc, node_mask) == 12);
    assert!(offset_of!(ComputePipelineStateDesc, cached_pso) == 16);
    assert!(offset_of!(ComputePipelineStateDesc, flags) == 24);
    assert!(size_of::<DepthStencilDesc>() == 52 && align_of::<DepthStencilDesc>() == 4);
    assert!(offset_of!(DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(DepthStencilDesc, back_face) == 36);
    assert!(size_of::<DepthStencilDesc1>() == 56 && align_of::<DepthStencilDesc1>() == 4);
    assert!(offset_of!(DepthStencilDesc1, depth_enable) == 0);
    assert!(offset_of!(DepthStencilDesc1, depth_write_mask) == 4);
    assert!(offset_of!(DepthStencilDesc1, depth_func) == 8);
    assert!(offset_of!(DepthStencilDesc1, stencil_enable) == 12);
    assert!(offset_of!(DepthStencilDesc1, stencil_read_mask) == 16);
    assert!(offset_of!(DepthStencilDesc1, stencil_write_mask) == 17);
    assert!(offset_of!(DepthStencilDesc1, front_face) == 20);
    assert!(offset_of!(DepthStencilDesc1, back_face) == 36);
    assert!(offset_of!(DepthStencilDesc1, depth_bounds_test_enable) == 52);
    assert!(size_of::<DepthStencilOpDesc>() == 16 && align_of::<DepthStencilOpDesc>() == 4);
    assert!(offset_of!(DepthStencilOpDesc, stencil_fail_op) == 0);
    assert!(offset_of!(DepthStencilOpDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(DepthStencilOpDesc, stencil_pass_op) == 8);
    assert!(offset_of!(DepthStencilOpDesc, stencil_func) == 12);
    assert!(
        size_of::<GraphicsPipelineStateDesc>() == 572
            && align_of::<GraphicsPipelineStateDesc>() == 4
    );
    assert!(offset_of!(GraphicsPipelineStateDesc, root_signature) == 0);
    assert!(offset_of!(GraphicsPipelineStateDesc, vs) == 4);
    assert!(offset_of!(GraphicsPipelineStateDesc, ps) == 12);
    assert!(offset_of!(GraphicsPipelineStateDesc, ds) == 20);
    assert!(offset_of!(GraphicsPipelineStateDesc, hs) == 28);
    assert!(offset_of!(GraphicsPipelineStateDesc, gs) == 36);
    assert!(offset_of!(GraphicsPipelineStateDesc, stream_output) == 44);
    assert!(offset_of!(GraphicsPipelineStateDesc, blend_state) == 64);
    assert!(offset_of!(GraphicsPipelineStateDesc, sample_mask) == 392);
    assert!(offset_of!(GraphicsPipelineStateDesc, rasterizer_state) == 396);
    assert!(offset_of!(GraphicsPipelineStateDesc, depth_stencil_state) == 440);
    assert!(offset_of!(GraphicsPipelineStateDesc, input_layout) == 492);
    assert!(offset_of!(GraphicsPipelineStateDesc, ib_strip_cut_value) == 500);
    assert!(offset_of!(GraphicsPipelineStateDesc, primitive_topology_type) == 504);
    assert!(offset_of!(GraphicsPipelineStateDesc, num_render_targets) == 508);
    assert!(offset_of!(GraphicsPipelineStateDesc, rtv_formats) == 512);
    assert!(offset_of!(GraphicsPipelineStateDesc, dsv_format) == 544);
    assert!(offset_of!(GraphicsPipelineStateDesc, sample_desc) == 548);
    assert!(offset_of!(GraphicsPipelineStateDesc, node_mask) == 556);
    assert!(offset_of!(GraphicsPipelineStateDesc, cached_pso) == 560);
    assert!(offset_of!(GraphicsPipelineStateDesc, flags) == 568);
    assert!(size_of::<InputElementDesc>() == 28 && align_of::<InputElementDesc>() == 4);
    assert!(offset_of!(InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(InputElementDesc, semantic_index) == 4);
    assert!(offset_of!(InputElementDesc, format) == 8);
    assert!(offset_of!(InputElementDesc, input_slot) == 12);
    assert!(offset_of!(InputElementDesc, aligned_byte_offset) == 16);
    assert!(offset_of!(InputElementDesc, input_slot_class) == 20);
    assert!(offset_of!(InputElementDesc, instance_data_step_rate) == 24);
    assert!(size_of::<InputLayoutDesc>() == 8 && align_of::<InputLayoutDesc>() == 4);
    assert!(offset_of!(InputLayoutDesc, input_element_descs) == 0);
    assert!(offset_of!(InputLayoutDesc, num_elements) == 4);
    assert!(
        size_of::<PipelineStateStreamDesc>() == 8 && align_of::<PipelineStateStreamDesc>() == 4
    );
    assert!(offset_of!(PipelineStateStreamDesc, size_in_bytes) == 0);
    assert!(offset_of!(PipelineStateStreamDesc, pipeline_state_subobject_stream) == 4);
    assert!(size_of::<RasterizerDesc>() == 44 && align_of::<RasterizerDesc>() == 4);
    assert!(offset_of!(RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(RasterizerDesc, multisample_enable) == 28);
    assert!(offset_of!(RasterizerDesc, antialiased_line_enable) == 32);
    assert!(offset_of!(RasterizerDesc, forced_sample_count) == 36);
    assert!(offset_of!(RasterizerDesc, conservative_raster) == 40);
    assert!(size_of::<RenderTargetBlendDesc>() == 40 && align_of::<RenderTargetBlendDesc>() == 4);
    assert!(offset_of!(RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(RenderTargetBlendDesc, logic_op_enable) == 4);
    assert!(offset_of!(RenderTargetBlendDesc, src_blend) == 8);
    assert!(offset_of!(RenderTargetBlendDesc, dest_blend) == 12);
    assert!(offset_of!(RenderTargetBlendDesc, blend_op) == 16);
    assert!(offset_of!(RenderTargetBlendDesc, src_blend_alpha) == 20);
    assert!(offset_of!(RenderTargetBlendDesc, dest_blend_alpha) == 24);
    assert!(offset_of!(RenderTargetBlendDesc, blend_op_alpha) == 28);
    assert!(offset_of!(RenderTargetBlendDesc, logic_op) == 32);
    assert!(offset_of!(RenderTargetBlendDesc, render_target_write_mask) == 36);
    assert!(size_of::<RtFormatArray>() == 36 && align_of::<RtFormatArray>() == 4);
    assert!(offset_of!(RtFormatArray, rt_formats) == 0);
    assert!(offset_of!(RtFormatArray, num_render_targets) == 32);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<ShaderBytecode>() == 8 && align_of::<ShaderBytecode>() == 4);
    assert!(offset_of!(ShaderBytecode, shader_bytecode) == 0);
    assert!(offset_of!(ShaderBytecode, bytecode_length) == 4);
    assert!(size_of::<SoDeclarationEntry>() == 16 && align_of::<SoDeclarationEntry>() == 4);
    assert!(offset_of!(SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(SoDeclarationEntry, semantic_name) == 4);
    assert!(offset_of!(SoDeclarationEntry, semantic_index) == 8);
    assert!(offset_of!(SoDeclarationEntry, start_component) == 12);
    assert!(offset_of!(SoDeclarationEntry, component_count) == 13);
    assert!(offset_of!(SoDeclarationEntry, output_slot) == 14);
    assert!(size_of::<StreamOutputDesc>() == 20 && align_of::<StreamOutputDesc>() == 4);
    assert!(offset_of!(StreamOutputDesc, so_declaration) == 0);
    assert!(offset_of!(StreamOutputDesc, num_entries) == 4);
    assert!(offset_of!(StreamOutputDesc, buffer_strides) == 8);
    assert!(offset_of!(StreamOutputDesc, num_strides) == 12);
    assert!(offset_of!(StreamOutputDesc, rasterized_stream) == 16);
    assert!(size_of::<ViewInstanceLocation>() == 8 && align_of::<ViewInstanceLocation>() == 4);
    assert!(offset_of!(ViewInstanceLocation, viewport_array_index) == 0);
    assert!(offset_of!(ViewInstanceLocation, render_target_array_index) == 4);
    assert!(size_of::<ViewInstancingDesc>() == 12 && align_of::<ViewInstancingDesc>() == 4);
    assert!(offset_of!(ViewInstancingDesc, view_instance_count) == 0);
    assert!(offset_of!(ViewInstancingDesc, view_instance_locations) == 4);
    assert!(offset_of!(ViewInstancingDesc, flags) == 8);
    assert!(size_of::<Subobject<*mut c_void>>() == 8 && align_of::<Subobject<*mut c_void>>() == 4);
    assert!(offset_of!(Subobject<*mut c_void>, data) == 4);
    assert!(
        size_of::<Subobject<ShaderBytecode>>() == 12
            && align_of::<Subobject<ShaderBytecode>>() == 4
    );
    assert!(offset_of!(Subobject<ShaderBytecode>, data) == 4);
    assert!(
        size_of::<Subobject<StreamOutputDesc>>() == 24
            && align_of::<Subobject<StreamOutputDesc>>() == 4
    );
    assert!(offset_of!(Subobject<StreamOutputDesc>, data) == 4);
    assert!(size_of::<Subobject<BlendDesc>>() == 332 && align_of::<Subobject<BlendDesc>>() == 4);
    assert!(offset_of!(Subobject<BlendDesc>, data) == 4);
    assert!(size_of::<Subobject<u32>>() == 8 && align_of::<Subobject<u32>>() == 4);
    assert!(offset_of!(Subobject<u32>, data) == 4);
    assert!(
        size_of::<Subobject<RasterizerDesc>>() == 48
            && align_of::<Subobject<RasterizerDesc>>() == 4
    );
    assert!(offset_of!(Subobject<RasterizerDesc>, data) == 4);
    assert!(
        size_of::<Subobject<DepthStencilDesc>>() == 56
            && align_of::<Subobject<DepthStencilDesc>>() == 4
    );
    assert!(offset_of!(Subobject<DepthStencilDesc>, data) == 4);
    assert!(
        size_of::<Subobject<InputLayoutDesc>>() == 12
            && align_of::<Subobject<InputLayoutDesc>>() == 4
    );
    assert!(offset_of!(Subobject<InputLayoutDesc>, data) == 4);
    assert!(
        size_of::<Subobject<RtFormatArray>>() == 40 && align_of::<Subobject<RtFormatArray>>() == 4
    );
    assert!(offset_of!(Subobject<RtFormatArray>, data) == 4);
    assert!(size_of::<Subobject<SampleDesc>>() == 12 && align_of::<Subobject<SampleDesc>>() == 4);
    assert!(offset_of!(Subobject<SampleDesc>, data) == 4);
    assert!(
        size_of::<Subobject<CachedPipelineState>>() == 12
            && align_of::<Subobject<CachedPipelineState>>() == 4
    );
    assert!(offset_of!(Subobject<CachedPipelineState>, data) == 4);
    assert!(
        size_of::<Subobject<DepthStencilDesc1>>() == 60
            && align_of::<Subobject<DepthStencilDesc1>>() == 4
    );
    assert!(offset_of!(Subobject<DepthStencilDesc1>, data) == 4);
    assert!(
        size_of::<Subobject<ViewInstancingDesc>>() == 16
            && align_of::<Subobject<ViewInstancingDesc>>() == 4
    );
    assert!(offset_of!(Subobject<ViewInstancingDesc>, data) == 4);
};
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<BlendDesc>() == 328 && align_of::<BlendDesc>() == 4);
    assert!(offset_of!(BlendDesc, alpha_to_coverage_enable) == 0);
    assert!(offset_of!(BlendDesc, independent_blend_enable) == 4);
    assert!(offset_of!(BlendDesc, render_target) == 8);
    assert!(size_of::<CachedPipelineState>() == 16 && align_of::<CachedPipelineState>() == 8);
    assert!(offset_of!(CachedPipelineState, cached_blob) == 0);
    assert!(offset_of!(CachedPipelineState, cached_blob_size_in_bytes) == 8);
    assert!(
        size_of::<ComputePipelineStateDesc>() == 56 && align_of::<ComputePipelineStateDesc>() == 8
    );
    assert!(offset_of!(ComputePipelineStateDesc, root_signature) == 0);
    assert!(offset_of!(ComputePipelineStateDesc, cs) == 8);
    assert!(offset_of!(ComputePipelineStateDesc, node_mask) == 24);
    assert!(offset_of!(ComputePipelineStateDesc, cached_pso) == 32);
    assert!(offset_of!(ComputePipelineStateDesc, flags) == 48);
    assert!(size_of::<DepthStencilDesc>() == 52 && align_of::<DepthStencilDesc>() == 4);
    assert!(offset_of!(DepthStencilDesc, depth_enable) == 0);
    assert!(offset_of!(DepthStencilDesc, depth_write_mask) == 4);
    assert!(offset_of!(DepthStencilDesc, depth_func) == 8);
    assert!(offset_of!(DepthStencilDesc, stencil_enable) == 12);
    assert!(offset_of!(DepthStencilDesc, stencil_read_mask) == 16);
    assert!(offset_of!(DepthStencilDesc, stencil_write_mask) == 17);
    assert!(offset_of!(DepthStencilDesc, front_face) == 20);
    assert!(offset_of!(DepthStencilDesc, back_face) == 36);
    assert!(size_of::<DepthStencilDesc1>() == 56 && align_of::<DepthStencilDesc1>() == 4);
    assert!(offset_of!(DepthStencilDesc1, depth_enable) == 0);
    assert!(offset_of!(DepthStencilDesc1, depth_write_mask) == 4);
    assert!(offset_of!(DepthStencilDesc1, depth_func) == 8);
    assert!(offset_of!(DepthStencilDesc1, stencil_enable) == 12);
    assert!(offset_of!(DepthStencilDesc1, stencil_read_mask) == 16);
    assert!(offset_of!(DepthStencilDesc1, stencil_write_mask) == 17);
    assert!(offset_of!(DepthStencilDesc1, front_face) == 20);
    assert!(offset_of!(DepthStencilDesc1, back_face) == 36);
    assert!(offset_of!(DepthStencilDesc1, depth_bounds_test_enable) == 52);
    assert!(size_of::<DepthStencilOpDesc>() == 16 && align_of::<DepthStencilOpDesc>() == 4);
    assert!(offset_of!(DepthStencilOpDesc, stencil_fail_op) == 0);
    assert!(offset_of!(DepthStencilOpDesc, stencil_depth_fail_op) == 4);
    assert!(offset_of!(DepthStencilOpDesc, stencil_pass_op) == 8);
    assert!(offset_of!(DepthStencilOpDesc, stencil_func) == 12);
    assert!(
        size_of::<GraphicsPipelineStateDesc>() == 656
            && align_of::<GraphicsPipelineStateDesc>() == 8
    );
    assert!(offset_of!(GraphicsPipelineStateDesc, root_signature) == 0);
    assert!(offset_of!(GraphicsPipelineStateDesc, vs) == 8);
    assert!(offset_of!(GraphicsPipelineStateDesc, ps) == 24);
    assert!(offset_of!(GraphicsPipelineStateDesc, ds) == 40);
    assert!(offset_of!(GraphicsPipelineStateDesc, hs) == 56);
    assert!(offset_of!(GraphicsPipelineStateDesc, gs) == 72);
    assert!(offset_of!(GraphicsPipelineStateDesc, stream_output) == 88);
    assert!(offset_of!(GraphicsPipelineStateDesc, blend_state) == 120);
    assert!(offset_of!(GraphicsPipelineStateDesc, sample_mask) == 448);
    assert!(offset_of!(GraphicsPipelineStateDesc, rasterizer_state) == 452);
    assert!(offset_of!(GraphicsPipelineStateDesc, depth_stencil_state) == 496);
    assert!(offset_of!(GraphicsPipelineStateDesc, input_layout) == 552);
    assert!(offset_of!(GraphicsPipelineStateDesc, ib_strip_cut_value) == 568);
    assert!(offset_of!(GraphicsPipelineStateDesc, primitive_topology_type) == 572);
    assert!(offset_of!(GraphicsPipelineStateDesc, num_render_targets) == 576);
    assert!(offset_of!(GraphicsPipelineStateDesc, rtv_formats) == 580);
    assert!(offset_of!(GraphicsPipelineStateDesc, dsv_format) == 612);
    assert!(offset_of!(GraphicsPipelineStateDesc, sample_desc) == 616);
    assert!(offset_of!(GraphicsPipelineStateDesc, node_mask) == 624);
    assert!(offset_of!(GraphicsPipelineStateDesc, cached_pso) == 632);
    assert!(offset_of!(GraphicsPipelineStateDesc, flags) == 648);
    assert!(size_of::<InputElementDesc>() == 32 && align_of::<InputElementDesc>() == 8);
    assert!(offset_of!(InputElementDesc, semantic_name) == 0);
    assert!(offset_of!(InputElementDesc, semantic_index) == 8);
    assert!(offset_of!(InputElementDesc, format) == 12);
    assert!(offset_of!(InputElementDesc, input_slot) == 16);
    assert!(offset_of!(InputElementDesc, aligned_byte_offset) == 20);
    assert!(offset_of!(InputElementDesc, input_slot_class) == 24);
    assert!(offset_of!(InputElementDesc, instance_data_step_rate) == 28);
    assert!(size_of::<InputLayoutDesc>() == 16 && align_of::<InputLayoutDesc>() == 8);
    assert!(offset_of!(InputLayoutDesc, input_element_descs) == 0);
    assert!(offset_of!(InputLayoutDesc, num_elements) == 8);
    assert!(
        size_of::<PipelineStateStreamDesc>() == 16 && align_of::<PipelineStateStreamDesc>() == 8
    );
    assert!(offset_of!(PipelineStateStreamDesc, size_in_bytes) == 0);
    assert!(offset_of!(PipelineStateStreamDesc, pipeline_state_subobject_stream) == 8);
    assert!(size_of::<RasterizerDesc>() == 44 && align_of::<RasterizerDesc>() == 4);
    assert!(offset_of!(RasterizerDesc, fill_mode) == 0);
    assert!(offset_of!(RasterizerDesc, cull_mode) == 4);
    assert!(offset_of!(RasterizerDesc, front_counter_clockwise) == 8);
    assert!(offset_of!(RasterizerDesc, depth_bias) == 12);
    assert!(offset_of!(RasterizerDesc, depth_bias_clamp) == 16);
    assert!(offset_of!(RasterizerDesc, slope_scaled_depth_bias) == 20);
    assert!(offset_of!(RasterizerDesc, depth_clip_enable) == 24);
    assert!(offset_of!(RasterizerDesc, multisample_enable) == 28);
    assert!(offset_of!(RasterizerDesc, antialiased_line_enable) == 32);
    assert!(offset_of!(RasterizerDesc, forced_sample_count) == 36);
    assert!(offset_of!(RasterizerDesc, conservative_raster) == 40);
    assert!(size_of::<RenderTargetBlendDesc>() == 40 && align_of::<RenderTargetBlendDesc>() == 4);
    assert!(offset_of!(RenderTargetBlendDesc, blend_enable) == 0);
    assert!(offset_of!(RenderTargetBlendDesc, logic_op_enable) == 4);
    assert!(offset_of!(RenderTargetBlendDesc, src_blend) == 8);
    assert!(offset_of!(RenderTargetBlendDesc, dest_blend) == 12);
    assert!(offset_of!(RenderTargetBlendDesc, blend_op) == 16);
    assert!(offset_of!(RenderTargetBlendDesc, src_blend_alpha) == 20);
    assert!(offset_of!(RenderTargetBlendDesc, dest_blend_alpha) == 24);
    assert!(offset_of!(RenderTargetBlendDesc, blend_op_alpha) == 28);
    assert!(offset_of!(RenderTargetBlendDesc, logic_op) == 32);
    assert!(offset_of!(RenderTargetBlendDesc, render_target_write_mask) == 36);
    assert!(size_of::<RtFormatArray>() == 36 && align_of::<RtFormatArray>() == 4);
    assert!(offset_of!(RtFormatArray, rt_formats) == 0);
    assert!(offset_of!(RtFormatArray, num_render_targets) == 32);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<ShaderBytecode>() == 16 && align_of::<ShaderBytecode>() == 8);
    assert!(offset_of!(ShaderBytecode, shader_bytecode) == 0);
    assert!(offset_of!(ShaderBytecode, bytecode_length) == 8);
    assert!(size_of::<SoDeclarationEntry>() == 24 && align_of::<SoDeclarationEntry>() == 8);
    assert!(offset_of!(SoDeclarationEntry, stream) == 0);
    assert!(offset_of!(SoDeclarationEntry, semantic_name) == 8);
    assert!(offset_of!(SoDeclarationEntry, semantic_index) == 16);
    assert!(offset_of!(SoDeclarationEntry, start_component) == 20);
    assert!(offset_of!(SoDeclarationEntry, component_count) == 21);
    assert!(offset_of!(SoDeclarationEntry, output_slot) == 22);
    assert!(size_of::<StreamOutputDesc>() == 32 && align_of::<StreamOutputDesc>() == 8);
    assert!(offset_of!(StreamOutputDesc, so_declaration) == 0);
    assert!(offset_of!(StreamOutputDesc, num_entries) == 8);
    assert!(offset_of!(StreamOutputDesc, buffer_strides) == 16);
    assert!(offset_of!(StreamOutputDesc, num_strides) == 24);
    assert!(offset_of!(StreamOutputDesc, rasterized_stream) == 28);
    assert!(size_of::<ViewInstanceLocation>() == 8 && align_of::<ViewInstanceLocation>() == 4);
    assert!(offset_of!(ViewInstanceLocation, viewport_array_index) == 0);
    assert!(offset_of!(ViewInstanceLocation, render_target_array_index) == 4);
    assert!(size_of::<ViewInstancingDesc>() == 24 && align_of::<ViewInstancingDesc>() == 8);
    assert!(offset_of!(ViewInstancingDesc, view_instance_count) == 0);
    assert!(offset_of!(ViewInstancingDesc, view_instance_locations) == 8);
    assert!(offset_of!(ViewInstancingDesc, flags) == 16);
    assert!(size_of::<Subobject<*mut c_void>>() == 16 && align_of::<Subobject<*mut c_void>>() == 8);
    assert!(offset_of!(Subobject<*mut c_void>, data) == 8);
    assert!(
        size_of::<Subobject<ShaderBytecode>>() == 24
            && align_of::<Subobject<ShaderBytecode>>() == 8
    );
    assert!(offset_of!(Subobject<ShaderBytecode>, data) == 8);
    assert!(
        size_of::<Subobject<StreamOutputDesc>>() == 40
            && align_of::<Subobject<StreamOutputDesc>>() == 8
    );
    assert!(offset_of!(Subobject<StreamOutputDesc>, data) == 8);
    assert!(size_of::<Subobject<BlendDesc>>() == 332 && align_of::<Subobject<BlendDesc>>() == 4);
    assert!(offset_of!(Subobject<BlendDesc>, data) == 4);
    assert!(size_of::<Subobject<u32>>() == 8 && align_of::<Subobject<u32>>() == 4);
    assert!(offset_of!(Subobject<u32>, data) == 4);
    assert!(
        size_of::<Subobject<RasterizerDesc>>() == 48
            && align_of::<Subobject<RasterizerDesc>>() == 4
    );
    assert!(offset_of!(Subobject<RasterizerDesc>, data) == 4);
    assert!(
        size_of::<Subobject<DepthStencilDesc>>() == 56
            && align_of::<Subobject<DepthStencilDesc>>() == 4
    );
    assert!(offset_of!(Subobject<DepthStencilDesc>, data) == 4);
    assert!(
        size_of::<Subobject<InputLayoutDesc>>() == 24
            && align_of::<Subobject<InputLayoutDesc>>() == 8
    );
    assert!(offset_of!(Subobject<InputLayoutDesc>, data) == 8);
    assert!(
        size_of::<Subobject<RtFormatArray>>() == 40 && align_of::<Subobject<RtFormatArray>>() == 4
    );
    assert!(offset_of!(Subobject<RtFormatArray>, data) == 4);
    assert!(size_of::<Subobject<SampleDesc>>() == 12 && align_of::<Subobject<SampleDesc>>() == 4);
    assert!(offset_of!(Subobject<SampleDesc>, data) == 4);
    assert!(
        size_of::<Subobject<CachedPipelineState>>() == 24
            && align_of::<Subobject<CachedPipelineState>>() == 8
    );
    assert!(offset_of!(Subobject<CachedPipelineState>, data) == 8);
    assert!(
        size_of::<Subobject<DepthStencilDesc1>>() == 60
            && align_of::<Subobject<DepthStencilDesc1>>() == 4
    );
    assert!(offset_of!(Subobject<DepthStencilDesc1>, data) == 4);
    assert!(
        size_of::<Subobject<ViewInstancingDesc>>() == 32
            && align_of::<Subobject<ViewInstancingDesc>>() == 8
    );
    assert!(offset_of!(Subobject<ViewInstancingDesc>, data) == 8);
};
